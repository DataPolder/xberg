use std::borrow::Cow;

use crate::{
    constants::DETECTION_CANVAS_PAD_PIXEL,
    ocr_error::OcrError,
    ocr_result::{Point, TextBox},
};
use image::imageops;
use imageproc::geometric_transformations::{Interpolation, Projection};
use ndarray::{Array, Array4};

const VERTICAL_TEXT_ASPECT_RATIO: f32 = 1.5;

pub struct OcrUtils;

impl OcrUtils {
    /// Normalize image pixels and transpose from HWC (row-major RGB) to CHW tensor format.
    ///
    /// Formula per pixel: `output[ch] = pixel[ch] * norm[ch] - mean[ch] * norm[ch]`
    ///
    /// This is a hot path called once per page. Key optimizations:
    /// - Pre-computes `mean * norm` constants (avoids repeated multiply)
    /// - Writes each channel plane contiguously via `as_slice_mut()`, enabling
    ///   LLVM auto-vectorization (NEON on ARM64, SSE/AVX on x86-64). The previous
    ///   approach used `tensor[[0, ch, r, c]]` which scattered writes across planes
    ///   and prevented any vectorization.
    pub fn substract_mean_normalize(img_src: &image::RgbImage, mean_vals: &[f32], norm_vals: &[f32]) -> Array4<f32> {
        let cols = img_src.width() as usize;
        let rows = img_src.height() as usize;
        let pixel_count = rows * cols;

        let mut input_tensor = Array::zeros((1, 3, rows, cols));

        let adjusted = [
            mean_vals[0] * norm_vals[0],
            mean_vals[1] * norm_vals[1],
            mean_vals[2] * norm_vals[2],
        ];

        let raw = img_src.as_raw();

        for ch in 0..3 {
            let norm = norm_vals[ch];
            let adj = adjusted[ch];
            let plane = input_tensor
                .slice_mut(ndarray::s![0, ch, .., ..])
                .into_shape_with_order(pixel_count)
                .expect("contiguous plane slice");
            let plane_slice = plane.into_slice().expect("contiguous memory");

            for (i, out) in plane_slice.iter_mut().enumerate() {
                *out = raw[i * 3 + ch] as f32 * norm - adj;
            }
        }

        input_tensor
    }

    /// Normalize an image into the **top-left** corner of a square `canvas x canvas` CHW
    /// tensor, filling the right/bottom remainder with normalized blank background.
    ///
    /// Used by `DbNet` when detection is pinned to a fixed canvas (the `tract` path): the
    /// image is *padded*, never stretched, so glyph aspect ratio is untouched and every real
    /// content coordinate keeps the value it has in the unpadded tensor. Padding sits only at
    /// right and bottom, so `ScaleParam`'s `scale_width`/`scale_height` back-mapping applies
    /// unchanged.
    ///
    /// Content pixels use the identical `pixel * norm - mean * norm` formula as
    /// [`Self::substract_mean_normalize`]; the padding uses that formula applied to
    /// [`DETECTION_CANVAS_PAD_PIXEL`], so padding is exactly what an all-white region
    /// normalizes to.
    ///
    /// Returns an error when the image does not fit the canvas -- the caller must size the
    /// canvas from the same limit that drives `ScaleParam::get_scale_param`.
    pub fn substract_mean_normalize_padded(
        img_src: &image::RgbImage,
        mean_vals: &[f32],
        norm_vals: &[f32],
        canvas: u32,
    ) -> Result<Array4<f32>, OcrError> {
        let cols = img_src.width() as usize;
        let rows = img_src.height() as usize;
        let canvas = canvas as usize;
        if cols > canvas || rows > canvas {
            return Err(OcrError::inference(format!(
                "detection canvas {canvas}x{canvas} is smaller than the resized page {cols}x{rows}"
            )));
        }

        let mut input_tensor = Array::zeros((1, 3, canvas, canvas));

        let adjusted = [
            mean_vals[0] * norm_vals[0],
            mean_vals[1] * norm_vals[1],
            mean_vals[2] * norm_vals[2],
        ];

        let raw = img_src.as_raw();

        for ch in 0..3 {
            let norm = norm_vals[ch];
            let adj = adjusted[ch];
            let plane = input_tensor
                .slice_mut(ndarray::s![0, ch, .., ..])
                .into_shape_with_order(canvas * canvas)
                .expect("contiguous plane slice");
            let plane_slice = plane.into_slice().expect("contiguous memory");

            plane_slice.fill(DETECTION_CANVAS_PAD_PIXEL * norm - adj);

            for row in 0..rows {
                let source_row = row * cols * 3;
                let destination = &mut plane_slice[row * canvas..row * canvas + cols];
                for (col, out) in destination.iter_mut().enumerate() {
                    *out = raw[source_row + col * 3 + ch] as f32 * norm - adj;
                }
            }
        }

        Ok(input_tensor)
    }

    /// Add white padding around the image, or borrow it unchanged when padding=0.
    /// Returns Cow to avoid cloning the image in the common no-padding case.
    pub fn make_padding<'a>(img_src: &'a image::RgbImage, padding: u32) -> Result<Cow<'a, image::RgbImage>, OcrError> {
        if padding == 0 {
            return Ok(Cow::Borrowed(img_src));
        }

        let width = img_src.width();
        let height = img_src.height();

        let mut padding_src = image::RgbImage::new(width + 2 * padding, height + 2 * padding);
        imageproc::drawing::draw_filled_rect_mut(
            &mut padding_src,
            imageproc::rect::Rect::at(0, 0).of_size(width + 2 * padding, height + 2 * padding),
            image::Rgb([255, 255, 255]),
        );

        image::imageops::replace(&mut padding_src, img_src, padding as i64, padding as i64);

        Ok(Cow::Owned(padding_src))
    }

    pub fn get_part_images(img_src: &image::RgbImage, text_boxes: &[TextBox]) -> Vec<image::RgbImage> {
        text_boxes
            .iter()
            .map(|text_box| Self::get_rotate_crop_image(img_src, &text_box.points))
            .collect()
    }

    pub fn get_rotate_crop_image(img_src: &image::RgbImage, box_points: &[Point]) -> image::RgbImage {
        let fallback = Self::bounding_crop(img_src, box_points);
        if box_points.len() < 4 {
            return fallback;
        }

        let edge_length = |first: Point, second: Point| {
            let dx = first.x as f32 - second.x as f32;
            let dy = first.y as f32 - second.y as f32;
            (dx * dx + dy * dy).sqrt()
        };
        let img_crop_width =
            edge_length(box_points[0], box_points[1]).max(edge_length(box_points[2], box_points[3])) as u32;
        let img_crop_height =
            edge_length(box_points[0], box_points[3]).max(edge_length(box_points[1], box_points[2])) as u32;

        if img_crop_width == 0 || img_crop_height == 0 {
            return fallback;
        }

        let src_points = [
            (box_points[0].x as f32, box_points[0].y as f32),
            (box_points[1].x as f32, box_points[1].y as f32),
            (box_points[2].x as f32, box_points[2].y as f32),
            (box_points[3].x as f32, box_points[3].y as f32),
        ];

        let dst_points = [
            (0.0, 0.0),
            (img_crop_width as f32, 0.0),
            (img_crop_width as f32, img_crop_height as f32),
            (0.0, img_crop_height as f32),
        ];

        let projection = match Projection::from_control_points(src_points, dst_points) {
            Some(proj) => proj,
            None => {
                return fallback;
            }
        };

        let mut part_img = image::RgbImage::new(img_crop_width, img_crop_height);
        imageproc::geometric_transformations::warp_into(
            img_src,
            projection,
            Interpolation::Bicubic,
            imageproc::geometric_transformations::Border::Replicate,
            &mut part_img,
        );

        if part_img.height() as f32 / part_img.width() as f32 >= VERTICAL_TEXT_ASPECT_RATIO {
            let mut rotated = image::RgbImage::new(part_img.height(), part_img.width());

            for (x, y, pixel) in part_img.enumerate_pixels() {
                rotated.put_pixel(y, part_img.width() - 1 - x, *pixel);
            }

            rotated
        } else {
            part_img
        }
    }

    fn bounding_crop(img_src: &image::RgbImage, points: &[Point]) -> image::RgbImage {
        let Some(first) = points.first() else {
            return image::RgbImage::new(0, 0);
        };
        let (min_x, min_y, max_x, max_y) = points.iter().skip(1).fold(
            (first.x, first.y, first.x, first.y),
            |(min_x, min_y, max_x, max_y), point| {
                (
                    min_x.min(point.x),
                    min_y.min(point.y),
                    max_x.max(point.x),
                    max_y.max(point.y),
                )
            },
        );
        let left = min_x.min(img_src.width());
        let top = min_y.min(img_src.height());
        let width = max_x.min(img_src.width()).saturating_sub(left);
        let height = max_y.min(img_src.height()).saturating_sub(top);
        imageops::crop_imm(img_src, left, top, width, height).to_image()
    }

    pub fn mat_rotate_clock_wise_180(src: &mut image::RgbImage) {
        imageops::rotate180_in_place(src);
    }

    /// Compute mean of f32 image values where mask > 0.
    ///
    /// Uses raw slice access instead of per-pixel get_pixel() for better
    /// cache behavior and to enable auto-vectorization of the reduction.
    pub fn calculate_mean_with_mask(
        img: &image::ImageBuffer<image::Luma<f32>, Vec<f32>>,
        mask: &image::ImageBuffer<image::Luma<u8>, Vec<u8>>,
    ) -> f32 {
        assert_eq!(img.width(), mask.width());
        assert_eq!(img.height(), mask.height());

        let img_raw = img.as_raw();
        let mask_raw = mask.as_raw();
        let mut sum: f32 = 0.0;
        let mut count: u32 = 0;

        for (px, &m) in img_raw.iter().zip(mask_raw.iter()) {
            if m > 0 {
                sum += *px;
                count += 1;
            }
        }

        if count == 0 { 0.0 } else { sum / count as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{IMAGENET_MEAN_VALUES, IMAGENET_NORM_VALUES};

    /// Value a white (blank-background) pixel normalizes to on channel `ch`.
    fn normalized_white(ch: usize) -> f32 {
        255.0 * IMAGENET_NORM_VALUES[ch] - IMAGENET_MEAN_VALUES[ch] * IMAGENET_NORM_VALUES[ch]
    }

    #[test]
    fn should_blit_content_into_the_canvas_top_left_and_pad_right_and_bottom() {
        let mut source = image::RgbImage::from_pixel(3, 2, image::Rgb([255, 255, 255]));
        source.put_pixel(0, 0, image::Rgb([10, 20, 30]));
        source.put_pixel(2, 1, image::Rgb([40, 50, 60]));
        let canvas = 5_u32;

        let unpadded = OcrUtils::substract_mean_normalize(&source, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES);
        let padded =
            OcrUtils::substract_mean_normalize_padded(&source, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES, canvas)
                .expect("the source fits the canvas");

        assert_eq!(padded.shape(), &[1, 3, canvas as usize, canvas as usize]);
        for ch in 0..3 {
            for row in 0..5 {
                for col in 0..5 {
                    let actual = padded[[0, ch, row, col]];
                    if row < 2 && col < 3 {
                        assert_eq!(
                            actual,
                            unpadded[[0, ch, row, col]],
                            "content pixel ({row},{col}) on channel {ch} must be unchanged by padding"
                        );
                    } else {
                        assert_eq!(
                            actual,
                            normalized_white(ch),
                            "pad pixel ({row},{col}) on channel {ch} must carry normalized blank background"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn should_fill_padding_with_the_value_a_white_pixel_normalizes_to() {
        let blank = image::RgbImage::from_pixel(1, 1, image::Rgb([255, 255, 255]));
        let reference = OcrUtils::substract_mean_normalize(&blank, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES);

        let content = image::RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 0]));
        let padded =
            OcrUtils::substract_mean_normalize_padded(&content, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES, 4)
                .expect("the source fits the canvas");

        for ch in 0..3 {
            assert_eq!(padded[[0, ch, 0, 3]], reference[[0, ch, 0, 0]]);
            assert_eq!(padded[[0, ch, 3, 0]], reference[[0, ch, 0, 0]]);
            assert_eq!(padded[[0, ch, 3, 3]], reference[[0, ch, 0, 0]]);
        }
    }

    #[test]
    fn should_reject_a_detection_canvas_smaller_than_the_resized_page() {
        let source = image::RgbImage::new(64, 96);

        let error =
            OcrUtils::substract_mean_normalize_padded(&source, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES, 64)
                .expect_err("a 96-row page cannot fit a 64x64 canvas");

        assert!(matches!(error, OcrError::Inference { .. }));
        assert!(error.to_string().contains("64x64"), "error was: {error}");
    }

    #[test]
    fn should_match_unpadded_normalization_when_the_canvas_is_exactly_the_image() {
        let mut source = image::RgbImage::from_pixel(2, 2, image::Rgb([7, 8, 9]));
        source.put_pixel(1, 1, image::Rgb([200, 100, 50]));

        let unpadded = OcrUtils::substract_mean_normalize(&source, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES);
        let padded =
            OcrUtils::substract_mean_normalize_padded(&source, &IMAGENET_MEAN_VALUES, &IMAGENET_NORM_VALUES, 2)
                .expect("the source fits the canvas");

        assert_eq!(padded, unpadded);
    }

    #[test]
    fn test_rotate_crop_uses_longest_opposing_edges() {
        let source = image::RgbImage::from_pixel(20, 20, image::Rgb([32, 64, 96]));
        let points = [
            Point { x: 2, y: 2 },
            Point { x: 12, y: 2 },
            Point { x: 16, y: 14 },
            Point { x: 2, y: 12 },
        ];

        let crop = OcrUtils::get_rotate_crop_image(&source, &points);

        assert_eq!(crop.dimensions(), (14, 12));
    }

    #[test]
    fn test_rotate_crop_replicates_source_at_warp_boundary() {
        let source = image::RgbImage::from_pixel(10, 10, image::Rgb([32, 64, 96]));
        let points = [
            Point { x: 0, y: 0 },
            Point { x: 9, y: 0 },
            Point { x: 9, y: 9 },
            Point { x: 0, y: 9 },
        ];

        let crop = OcrUtils::get_rotate_crop_image(&source, &points);

        assert!(crop.pixels().all(|pixel| *pixel == image::Rgb([32, 64, 96])));
    }

    #[test]
    fn test_rotate_crop_does_not_rotate_below_vertical_threshold() {
        let source = image::RgbImage::from_pixel(8, 8, image::Rgb([32, 64, 96]));
        let points = [
            Point { x: 0, y: 0 },
            Point { x: 3, y: 0 },
            Point { x: 3, y: 4 },
            Point { x: 0, y: 4 },
        ];

        let crop = OcrUtils::get_rotate_crop_image(&source, &points);

        assert_eq!(crop.dimensions(), (3, 4));
    }
}
