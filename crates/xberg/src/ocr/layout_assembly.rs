//! Layout-aware OCR table recognition.
//!
//! This module provides TATR-based table structure recognition for OCR pages.
//! It operates entirely in pixel space — no coordinate conversion is needed
//! because both OCR elements and layout detections use the same image
//! coordinate system (origin top-left, y increases downward).

use crate::layout::models::tatr::{self, TatrModel};
use crate::layout::types::{BBox, DetectionResult, LayoutClass, RecognizedTable};
use crate::types::OcrElement;

/// Default confidence threshold for layout detections.
const MIN_CONFIDENCE: f32 = 0.3;

/// Minimum intersection-over-word-area required to assign an OCR element to a table cell.
const MIN_CELL_ELEMENT_IOW: f32 = 0.2;

/// Run TATR table recognition for all Table regions in a page.
///
/// For each Table detection, crops the page image, runs TATR inference,
/// matches OCR elements to cells, and produces markdown tables.
pub(crate) fn recognize_page_tables(
    page_image: &image::RgbImage,
    detection: &DetectionResult,
    elements: &[OcrElement],
    tatr_model: &mut TatrModel,
) -> Vec<RecognizedTable> {
    let mut tables = Vec::new();

    for det in &detection.detections {
        if det.class_name != LayoutClass::Table || det.confidence < MIN_CONFIDENCE {
            continue;
        }

        let result = recognize_single_table(page_image, &det.bbox, elements, tatr_model);
        if let Some((cells, markdown)) = result {
            tables.push(RecognizedTable {
                detection_bbox: det.bbox,
                cells,
                markdown,
            });
        }
    }

    tables
}

/// Recognize a single table from a cropped region of the page.
///
/// Returns `(cells, markdown)` where cells is the 2D grid of cell text content.
fn recognize_single_table(
    page_image: &image::RgbImage,
    table_bbox: &BBox,
    elements: &[OcrElement],
    tatr_model: &mut TatrModel,
) -> Option<(Vec<Vec<String>>, String)> {
    let crop_x = table_bbox.x1.max(0.0) as u32;
    let crop_y = table_bbox.y1.max(0.0) as u32;
    let crop_w = (table_bbox.width() as u32).min(page_image.width().saturating_sub(crop_x));
    let crop_h = (table_bbox.height() as u32).min(page_image.height().saturating_sub(crop_y));

    if crop_w == 0 || crop_h == 0 {
        return None;
    }

    let cropped = image::imageops::crop_imm(page_image, crop_x, crop_y, crop_w, crop_h).to_image();

    let tatr_result = match tatr_model.recognize(&cropped) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("TATR inference failed: {e}");
            return None;
        }
    };

    if tatr_result.rows.is_empty() || tatr_result.columns.is_empty() {
        return None;
    }

    let cell_grid = tatr::build_cell_grid(&tatr_result, None);
    if cell_grid.is_empty() || cell_grid[0].is_empty() {
        return None;
    }

    if !is_cell_grid_valid(&cell_grid) {
        tracing::debug!("TATR cell grid is invalid (too many empty cells or malformed); skipping table");
        return None;
    }

    let table_elements: Vec<&OcrElement> = elements
        .iter()
        .filter(|e| {
            if e.text.trim().is_empty() {
                return false;
            }
            element_bbox_iow(e, table_bbox) >= MIN_CELL_ELEMENT_IOW
        })
        .collect();

    let (cells, markdown) = build_markdown_table(&cell_grid, &table_elements, crop_x as f32, crop_y as f32);
    Some((cells, markdown))
}

/// Build a markdown table from TATR cell grid + OCR elements.
///
/// Cell bboxes from TATR are in cropped-image coordinates.
/// OCR elements are in page coordinates. `offset_x/y` translates between them.
fn build_markdown_table(
    cell_grid: &[Vec<tatr::CellBBox>],
    elements: &[&OcrElement],
    offset_x: f32,
    offset_y: f32,
) -> (Vec<Vec<String>>, String) {
    if cell_grid.is_empty() {
        return (Vec::new(), String::new());
    }

    let num_cols = cell_grid[0].len();

    if num_cols == 0 {
        return (Vec::new(), String::new());
    }

    let mut assigned = assign_elements_to_best_cells(cell_grid, elements, offset_x, offset_y, num_cols);
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(assigned.len());
    for row in &mut assigned {
        let mut grid_row = vec![String::new(); num_cols];
        for (column, cell_elements) in row.iter_mut().enumerate() {
            grid_row[column] = text_from_assigned_elements(cell_elements);
        }
        grid.push(grid_row);
    }

    let mut md = String::new();

    for (row_idx, row) in grid.iter().enumerate() {
        md.push('|');
        for cell in row {
            let escaped = cell.replace('|', "\\|");
            md.push(' ');
            md.push_str(escaped.trim());
            md.push_str(" |");
        }
        md.push('\n');

        if row_idx == 0 {
            md.push('|');
            for _ in 0..num_cols {
                md.push_str(" --- |");
            }
            md.push('\n');
        }
    }

    if md.ends_with('\n') {
        md.pop();
    }

    (grid, md)
}

type PositionedElement<'a> = (&'a OcrElement, f32, f32);
type CellAssignments<'a> = Vec<Vec<Vec<PositionedElement<'a>>>>;

/// Assign every OCR element to its single highest-IoW cell across the full grid.
/// Equal overlaps keep the first cell in row-major order.
fn assign_elements_to_best_cells<'a>(
    cell_grid: &[Vec<tatr::CellBBox>],
    elements: &[&'a OcrElement],
    offset_x: f32,
    offset_y: f32,
    num_cols: usize,
) -> CellAssignments<'a> {
    let page_cells = cell_grid
        .iter()
        .map(|row| {
            row.iter()
                .take(num_cols)
                .map(|cell| {
                    BBox::new(
                        cell.x1 + offset_x,
                        cell.y1 + offset_y,
                        cell.x2 + offset_x,
                        cell.y2 + offset_y,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut assigned: CellAssignments<'a> = page_cells.iter().map(|row| vec![Vec::new(); row.len()]).collect();

    for &element in elements {
        let mut best_iow = 0.0;
        let mut best_cell = None;
        for (row, cells) in page_cells.iter().enumerate() {
            for (column, cell) in cells.iter().enumerate() {
                let iow = element_bbox_iow(element, cell);
                if iow > best_iow {
                    best_iow = iow;
                    best_cell = Some((row, column));
                }
            }
        }
        if best_iow >= MIN_CELL_ELEMENT_IOW
            && let Some((row, column)) = best_cell
        {
            let (center_x, center_y) = element_center_f32(element);
            assigned[row][column].push((element, center_x, center_y));
        }
    }
    assigned
}

/// Assemble uniquely assigned elements in top-to-bottom, left-to-right order.
fn text_from_assigned_elements(elements: &mut [PositionedElement<'_>]) -> String {
    if elements.is_empty() {
        return String::new();
    }
    elements.sort_by(|left, right| left.2.total_cmp(&right.2).then_with(|| left.1.total_cmp(&right.1)));
    elements
        .iter()
        .map(|(element, _, _)| element.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compute intersection-over-word-area (IoW) between an OCR element and a BBox.
///
/// Returns the fraction of the element's area that overlaps with the given bbox.
/// For zero-area elements, falls back to center-point containment (returns 0.0 or 1.0).
fn element_bbox_iow(elem: &OcrElement, bbox: &BBox) -> f32 {
    let (left, top, width, height) = elem.geometry.to_aabb();
    let e_left = left as f32;
    let e_top = top as f32;
    let e_right = e_left + width as f32;
    let e_bottom = e_top + height as f32;
    let elem_area = width as f32 * height as f32;

    if elem_area <= 0.0 {
        let cx = e_left + width as f32 / 2.0;
        let cy = e_top + height as f32 / 2.0;
        return if point_in_bbox(cx, cy, bbox) { 1.0 } else { 0.0 };
    }

    let inter_left = e_left.max(bbox.x1);
    let inter_top = e_top.max(bbox.y1);
    let inter_right = e_right.min(bbox.x2);
    let inter_bottom = e_bottom.min(bbox.y2);
    let inter_area = (inter_right - inter_left).max(0.0) * (inter_bottom - inter_top).max(0.0);

    inter_area / elem_area
}

/// Get element center as f32 (for matching with BBox which uses f32).
fn element_center_f32(elem: &OcrElement) -> (f32, f32) {
    let (cx, cy) = elem.geometry.center();
    (cx as f32, cy as f32)
}

/// Check if a point (cx, cy) is inside a BBox (pixel coords: y increases downward).
fn point_in_bbox(cx: f32, cy: f32, bbox: &BBox) -> bool {
    cx >= bbox.x1 && cx <= bbox.x2 && cy >= bbox.y1 && cy <= bbox.y2
}

/// Validate TATR cell grid sanity.
///
/// Detects malformed tables from low-confidence TATR output (category C).
/// Returns false if:
/// - More than 30% of cells are empty (indicates bad segmentation)
/// - Grid has < 2 rows or < 2 columns (degenerate)
fn is_cell_grid_valid(cell_grid: &[Vec<tatr::CellBBox>]) -> bool {
    if cell_grid.len() < 2 {
        return false;
    }
    if cell_grid[0].len() < 2 {
        return false;
    }

    let mut empty_count = 0;
    let total_count = cell_grid.len() * cell_grid[0].len();

    for row in cell_grid {
        for cell in row {
            let width = (cell.x2 - cell.x1).abs();
            let height = (cell.y2 - cell.y1).abs();
            if width < 1.0 || height < 1.0 {
                empty_count += 1;
            }
        }
    }

    let empty_ratio = empty_count as f32 / total_count as f32;
    if empty_ratio > 0.3 {
        tracing::debug!(
            empty_count,
            total_count,
            empty_ratio,
            "TATR cell grid has too many empty cells ({:.1}%)",
            empty_ratio * 100.0
        );
        return false;
    }

    true
}

#[cfg(all(test, feature = "ocr"))]
mod tests {
    use super::*;
    use crate::types::{OcrBoundingGeometry, OcrConfidence, OcrElementLevel};

    fn cell(x1: f32, y1: f32, x2: f32, y2: f32) -> tatr::CellBBox {
        tatr::CellBBox { x1, y1, x2, y2 }
    }

    fn word(text: &str, left: u32, top: u32, width: u32, height: u32) -> OcrElement {
        OcrElement::new(
            text,
            OcrBoundingGeometry::Rectangle {
                left,
                top,
                width,
                height,
            },
            OcrConfidence::from_tesseract(95.0),
        )
        .with_level(OcrElementLevel::Word)
    }

    #[test]
    fn should_assign_ordinary_grid_elements_in_reading_order() {
        let grid = vec![
            vec![cell(0.0, 0.0, 50.0, 50.0), cell(50.0, 0.0, 100.0, 50.0)],
            vec![cell(0.0, 50.0, 50.0, 100.0), cell(50.0, 50.0, 100.0, 100.0)],
        ];
        let elements = [
            word("B", 70, 10, 10, 10),
            word("two", 25, 10, 10, 10),
            word("one", 10, 10, 10, 10),
            word("D", 70, 70, 10, 10),
            word("C", 10, 70, 10, 10),
        ];
        let element_refs = elements.iter().collect::<Vec<_>>();

        let (cells, markdown) = build_markdown_table(&grid, &element_refs, 0.0, 0.0);

        assert_eq!(cells, vec![vec!["one two", "B"], vec!["C", "D"]]);
        assert_eq!(markdown, "| one two | B |\n| --- | --- |\n| C | D |");
    }

    #[test]
    fn should_assign_overlapping_element_only_to_highest_iow_cell() {
        let grid = vec![vec![cell(0.0, 0.0, 60.0, 50.0), cell(40.0, 0.0, 100.0, 50.0)]];
        let elements = [word("overlap", 50, 10, 20, 10)];
        let element_refs = elements.iter().collect::<Vec<_>>();

        let (cells, _) = build_markdown_table(&grid, &element_refs, 0.0, 0.0);

        assert_eq!(cells, vec![vec!["", "overlap"]]);
    }

    #[test]
    fn should_break_equal_iow_ties_by_row_then_column() {
        let grid = vec![vec![cell(0.0, 0.0, 60.0, 50.0), cell(40.0, 0.0, 100.0, 50.0)]];
        let elements = [word("tie", 45, 10, 10, 10)];
        let element_refs = elements.iter().collect::<Vec<_>>();

        let (cells, _) = build_markdown_table(&grid, &element_refs, 0.0, 0.0);

        assert_eq!(cells, vec![vec!["tie", ""]]);
    }

    #[test]
    fn should_assign_zero_area_element_by_center_without_duplication() {
        let grid = vec![vec![cell(0.0, 0.0, 50.0, 50.0), cell(50.0, 0.0, 100.0, 50.0)]];
        let elements = [word("point", 75, 25, 0, 0)];
        let element_refs = elements.iter().collect::<Vec<_>>();

        let (cells, _) = build_markdown_table(&grid, &element_refs, 0.0, 0.0);

        assert_eq!(cells, vec![vec!["", "point"]]);
    }

    #[test]
    fn should_emit_spanning_cell_element_once_for_repeated_boxes() {
        let spanning = cell(0.0, 0.0, 100.0, 50.0);
        let grid = vec![vec![spanning, spanning]];
        let elements = [word("span", 40, 10, 20, 10)];
        let element_refs = elements.iter().collect::<Vec<_>>();

        let (cells, _) = build_markdown_table(&grid, &element_refs, 0.0, 0.0);

        assert_eq!(cells, vec![vec!["span", ""]]);
    }
}
