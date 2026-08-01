use crate::{
    base_net::BaseNet,
    constants::{IMAGENET_MEAN_VALUES, IMAGENET_NORM_VALUES},
    ocr_error::OcrError,
    ocr_result::{self, TextBox},
    ocr_utils::OcrUtils,
    scale_param::ScaleParam,
};
use geo_clipper::{Clipper, EndType, JoinType};
use geo_types::{Coord, LineString, Polygon};
use ort::{inputs, session::SessionOutputs};
use ort::{session::Session, value::Tensor};
use std::cmp::Ordering;

const DB_DILATION_KERNEL_SIZE: u32 = 2;
const BINARY_FOREGROUND: u8 = u8::MAX;

#[derive(Debug)]
pub struct DbNet {
    session: Option<Session>,
    input_names: Vec<String>,
}

impl BaseNet for DbNet {
    fn new() -> Self {
        Self {
            session: None,
            input_names: Vec::new(),
        }
    }

    fn set_input_names(&mut self, input_names: Vec<String>) {
        self.input_names = input_names;
    }

    fn set_session(&mut self, session: Option<Session>) {
        self.session = session;
    }
}

impl DbNet {
    pub fn get_text_boxes(
        &self,
        img_src: &image::RgbImage,
        scale: &ScaleParam,
        box_score_thresh: f32,
        box_thresh: f32,
        un_clip_ratio: f32,
    ) -> Result<Vec<TextBox>, OcrError> {
        let Some(session) = &self.session else {
            return Err(OcrError::SessionNotInitialized);
        };

        let src_resize = image::imageops::resize(
            img_src,
            scale.dst_width,
            scale.dst_height,
            image::imageops::FilterType::Triangle,
        );

        let input_tensors =
            OcrUtils::substract_mean_normalize(&src_resize, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES);

        let tensor = Tensor::from_array(input_tensors)?;

        #[allow(unsafe_code)]
        let outputs = unsafe {
            let session_ptr = session as *const Session as *mut Session;
            (*session_ptr).run(inputs![self.input_names[0].as_str() => tensor])?
        };

        let text_boxes = Self::get_text_boxes_core(
            &outputs,
            src_resize.height(),
            src_resize.width(),
            &ScaleParam::new(
                scale.src_width,
                scale.src_height,
                scale.dst_width,
                scale.dst_height,
                scale.scale_width,
                scale.scale_height,
            ),
            box_score_thresh,
            box_thresh,
            un_clip_ratio,
        )?;

        Ok(text_boxes)
    }

    fn get_text_boxes_core(
        output_tensor: &SessionOutputs,
        rows: u32,
        cols: u32,
        s: &ScaleParam,
        box_score_thresh: f32,
        box_thresh: f32,
        un_clip_ratio: f32,
    ) -> Result<Vec<TextBox>, OcrError> {
        let max_side_thresh = 3.0;

        let (_, red_data) = output_tensor.iter().next().ok_or_else(|| {
            OcrError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No output tensors found in session output",
            ))
        })?;

        let pred_data: Vec<f32> = red_data.try_extract_tensor::<f32>()?.1.to_vec();

        let threshold_img = Self::binarize_predictions(&pred_data, rows, cols, box_thresh)?;
        let dilated_img = Self::dilate_db_mask(&threshold_img);

        let pred_img: image::ImageBuffer<image::Luma<f32>, Vec<f32>> =
            image::ImageBuffer::from_vec(cols, rows, pred_data).ok_or_else(|| {
                OcrError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "Failed to create image buffer from predictions: {} x {} dimensions may be invalid",
                        cols, rows
                    ),
                ))
            })?;

        let img_contours: Vec<imageproc::contours::Contour<i32>> = imageproc::contours::find_contours(&dilated_img);

        let mut rs_boxes = Vec::with_capacity(img_contours.len());

        for contour in img_contours {
            if contour.points.len() <= 2 {
                continue;
            }

            let mut max_side = 0.0;
            let min_box = Self::get_mini_box(&contour.points, &mut max_side)?;
            if max_side < max_side_thresh {
                continue;
            }

            let score = Self::get_score(&contour, &pred_img)?;
            if score < box_score_thresh {
                continue;
            }

            let clip_box = Self::unclip(&min_box, un_clip_ratio)?;
            if clip_box.is_empty() {
                continue;
            }

            let mut clip_contour = Vec::new();
            for point in &clip_box {
                clip_contour.push(*point);
            }

            let mut max_side_clip = 0.0;
            let clip_min_box = Self::get_mini_box(&clip_contour, &mut max_side_clip)?;
            if max_side_clip < max_side_thresh + 2.0 {
                continue;
            }

            let mut final_points = Vec::new();
            for item in clip_min_box {
                let x = (item.x / s.scale_width) as u32;
                let ptx = x.min(s.src_width);

                let y = (item.y / s.scale_height) as u32;
                let pty = y.min(s.src_height);

                final_points.push(ocr_result::Point { x: ptx, y: pty });
            }

            let text_box = TextBox {
                score,
                points: final_points,
            };

            rs_boxes.push(text_box);
        }

        Ok(rs_boxes)
    }

    fn binarize_predictions(
        predictions: &[f32],
        rows: u32,
        cols: u32,
        threshold: f32,
    ) -> Result<image::GrayImage, OcrError> {
        let binary_data = predictions
            .iter()
            .map(|&prediction| if prediction > threshold { BINARY_FOREGROUND } else { 0 })
            .collect();

        image::GrayImage::from_vec(cols, rows, binary_data).ok_or_else(|| {
            OcrError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Failed to create DB mask: {} x {} dimensions do not match predictions",
                    cols, rows
                ),
            ))
        })
    }

    fn dilate_db_mask(mask: &image::GrayImage) -> image::GrayImage {
        let mut dilated = image::GrayImage::new(mask.width(), mask.height());
        for (x, y, pixel) in mask.enumerate_pixels() {
            if pixel.0[0] == 0 {
                continue;
            }

            for offset_y in 0..DB_DILATION_KERNEL_SIZE {
                for offset_x in 0..DB_DILATION_KERNEL_SIZE {
                    if let Some(destination) = dilated.get_pixel_mut_checked(x + offset_x, y + offset_y) {
                        *destination = image::Luma([BINARY_FOREGROUND]);
                    }
                }
            }
        }
        dilated
    }

    fn get_mini_box(
        contour_points: &[imageproc::point::Point<i32>],
        min_edge_size: &mut f32,
    ) -> Result<Vec<imageproc::point::Point<f32>>, OcrError> {
        let rect = imageproc::geometry::min_area_rect(contour_points);

        let mut rect_points: Vec<imageproc::point::Point<f32>> = rect
            .iter()
            .map(|p| imageproc::point::Point::new(p.x as f32, p.y as f32))
            .collect();

        let dx_w = rect_points[0].x - rect_points[1].x;
        let dy_w = rect_points[0].y - rect_points[1].y;
        let width = (dx_w * dx_w + dy_w * dy_w).sqrt();
        let dx_h = rect_points[1].x - rect_points[2].x;
        let dy_h = rect_points[1].y - rect_points[2].y;
        let height = (dx_h * dx_h + dy_h * dy_h).sqrt();

        *min_edge_size = width.min(height);

        rect_points.sort_by(|a, b| {
            if a.x > b.x {
                return Ordering::Greater;
            }
            if a.x == b.x {
                return Ordering::Equal;
            }
            Ordering::Less
        });

        let mut box_points = Vec::new();
        let index_1;
        let index_4;
        if rect_points[1].y > rect_points[0].y {
            index_1 = 0;
            index_4 = 1;
        } else {
            index_1 = 1;
            index_4 = 0;
        }

        let index_2;
        let index_3;
        if rect_points[3].y > rect_points[2].y {
            index_2 = 2;
            index_3 = 3;
        } else {
            index_2 = 3;
            index_3 = 2;
        }

        box_points.push(rect_points[index_1]);
        box_points.push(rect_points[index_2]);
        box_points.push(rect_points[index_3]);
        box_points.push(rect_points[index_4]);

        Ok(box_points)
    }

    fn get_score(
        contour: &imageproc::contours::Contour<i32>,
        f_map_mat: &image::ImageBuffer<image::Luma<f32>, Vec<f32>>,
    ) -> Result<f32, OcrError> {
        let mut xmin = i32::MAX;
        let mut xmax = i32::MIN;
        let mut ymin = i32::MAX;
        let mut ymax = i32::MIN;

        for point in contour.points.iter() {
            let x = point.x;
            let y = point.y;

            if x < xmin {
                xmin = x;
            }
            if x > xmax {
                xmax = x;
            }
            if y < ymin {
                ymin = y;
            }
            if y > ymax {
                ymax = y;
            }
        }

        let width = f_map_mat.width() as i32;
        let height = f_map_mat.height() as i32;

        xmin = xmin.max(0).min(width - 1);
        xmax = xmax.max(0).min(width - 1);
        ymin = ymin.max(0).min(height - 1);
        ymax = ymax.max(0).min(height - 1);

        let roi_width = xmax - xmin + 1;
        let roi_height = ymax - ymin + 1;

        if roi_width <= 0 || roi_height <= 0 {
            return Ok(0.0);
        }

        let mut mask = image::GrayImage::new(roi_width as u32, roi_height as u32);

        let mut pts = Vec::<imageproc::point::Point<i32>>::new();
        for point in contour.points.iter() {
            pts.push(imageproc::point::Point::new(point.x - xmin, point.y - ymin));
        }

        imageproc::drawing::draw_polygon_mut(&mut mask, pts.as_slice(), image::Luma([255]));

        let cropped_img =
            image::imageops::crop_imm(f_map_mat, xmin as u32, ymin as u32, roi_width as u32, roi_height as u32)
                .to_image();

        let mean = OcrUtils::calculate_mean_with_mask(&cropped_img, &mask);

        Ok(mean)
    }

    fn unclip(
        box_points: &[imageproc::point::Point<f32>],
        unclip_ratio: f32,
    ) -> Result<Vec<imageproc::point::Point<i32>>, OcrError> {
        let dx_w = box_points[0].x - box_points[1].x;
        let dy_w = box_points[0].y - box_points[1].y;
        let clip_rect_width = (dx_w * dx_w + dy_w * dy_w).sqrt();
        let dx_h = box_points[1].x - box_points[2].x;
        let dy_h = box_points[1].y - box_points[2].y;
        let clip_rect_height = (dx_h * dx_h + dy_h * dy_h).sqrt();

        if clip_rect_height < 1.001 && clip_rect_width < 1.001 {
            return Ok(Vec::new());
        }

        let mut the_cliper_pts = Vec::new();
        for pt in box_points {
            let a1 = Coord {
                x: pt.x as f64,
                y: pt.y as f64,
            };
            the_cliper_pts.push(a1);
        }

        let area = Self::signed_polygon_area(box_points).abs();
        let length = Self::length_of_points(box_points);
        let distance = area * unclip_ratio / length as f32;

        let co = Polygon::new(LineString::new(the_cliper_pts), vec![]);
        let solution = co
            .offset(distance as f64, JoinType::Round(2.0), EndType::ClosedPolygon, 1.0)
            .0;

        if solution.is_empty() {
            return Ok(Vec::new());
        }

        let first_polygon = solution.first().ok_or_else(|| {
            OcrError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Polygon solution list was empty after offset operation",
            ))
        })?;

        let ret_pts: Vec<_> = first_polygon
            .exterior()
            .points()
            .map(|ip| imageproc::point::Point::new(ip.x() as i32, ip.y() as i32))
            .collect();

        Ok(ret_pts)
    }

    fn signed_polygon_area(points: &[imageproc::point::Point<f32>]) -> f32 {
        let num_points = points.len();
        let mut pts = Vec::with_capacity(num_points + 1);
        pts.extend_from_slice(points);
        pts.push(points[0]);

        let mut area = 0.0;
        for i in 0..num_points {
            area += (pts[i + 1].x - pts[i].x) * (pts[i + 1].y + pts[i].y) / 2.0;
        }

        area
    }

    fn length_of_points(box_points: &[imageproc::point::Point<f32>]) -> f64 {
        if box_points.is_empty() {
            return 0.0;
        }

        let mut length = 0.0;
        let mut x0 = box_points[0].x as f64;
        let mut y0 = box_points[0].y as f64;

        for pt in &box_points[1..] {
            let x1 = pt.x as f64;
            let y1 = pt.y as f64;
            let dx = x1 - x0;
            let dy = y1 - y0;
            length += (dx * dx + dy * dy).sqrt();
            x0 = x1;
            y0 = y1;
        }

        let dx = box_points[0].x as f64 - x0;
        let dy = box_points[0].y as f64 - y0;
        length += (dx * dx + dy * dy).sqrt();

        length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_configured_db_threshold_when_binarizing_predictions() {
        let predictions = [0.29, 0.30, 0.31, 0.80];

        let mask = DbNet::binarize_predictions(&predictions, 1, 4, 0.30).unwrap();

        assert_eq!(mask.as_raw(), &[0, 0, BINARY_FOREGROUND, BINARY_FOREGROUND]);
    }

    #[test]
    fn should_apply_rapidocr_two_by_two_dilation_to_db_mask() {
        let mut mask = image::GrayImage::new(4, 4);
        mask.put_pixel(1, 1, image::Luma([BINARY_FOREGROUND]));

        let dilated = DbNet::dilate_db_mask(&mask);

        assert_eq!(
            dilated.as_raw(),
            &[
                0,
                0,
                0,
                0,
                0,
                BINARY_FOREGROUND,
                BINARY_FOREGROUND,
                0,
                0,
                BINARY_FOREGROUND,
                BINARY_FOREGROUND,
                0,
                0,
                0,
                0,
                0,
            ]
        );
    }
}
