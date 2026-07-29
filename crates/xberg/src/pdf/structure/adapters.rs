//! OCR-to-structure adapters: convert xberg internal types into the PDF
//! structure pipeline's paragraph representation.
#[cfg(feature = "ocr")]
use super::types;

/// Convert an OCR-produced [`crate::types::internal::InternalDocument`] into a vec of [`types::PdfParagraph`]s
/// for the structure assembly pipeline.
///
/// Coordinates are in image-space (y=0 at top) and are flipped to PDF-space
/// (y=0 at bottom) using `page_height_px`.
#[cfg(feature = "ocr")]
#[allow(dead_code)]
pub(crate) fn ocr_doc_to_paragraphs(
    doc: &crate::types::internal::InternalDocument,
    page_height_px: u32,
) -> Vec<types::PdfParagraph> {
    use crate::types::internal::ElementKind;
    let page_h = page_height_px as f32;
    let result = doc
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::OcrText { .. }))
        .filter(|e| !e.text.trim().is_empty())
        .map(|element| make_ocr_block_paragraph(element, page_h))
        .collect::<Vec<_>>();

    trace_conversion(doc, &result);
    result
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
pub(crate) fn ocr_doc_to_layout_paragraphs(
    doc: &crate::types::internal::InternalDocument,
    page_height_px: u32,
    hints: &[types::LayoutHint],
    min_confidence: f32,
    min_containment: f32,
) -> Vec<types::PdfParagraph> {
    use crate::types::internal::ElementKind;
    let page_height = page_height_px as f32;
    let mut result = Vec::new();

    for element in doc
        .elements
        .iter()
        .filter(|element| matches!(element.kind, ElementKind::OcrText { .. }))
        .filter(|element| !element.text.trim().is_empty())
    {
        let mut lines = make_ocr_line_paragraphs(element, page_height);
        let selected = super::layout_classify::apply_layout_overrides_with_matches(
            &mut lines,
            hints,
            min_confidence,
            min_containment,
            None,
        );
        let hint_indices = compatible_hint_indices(&lines, hints, selected, min_containment);
        result.extend(regroup_layout_lines(lines, hint_indices));
    }

    trace_conversion(doc, &result);
    result
}

#[cfg(feature = "ocr")]
fn trace_conversion(doc: &crate::types::internal::InternalDocument, result: &[types::PdfParagraph]) {
    tracing::debug!(
        input_elements = doc
            .elements
            .iter()
            .filter(|element| matches!(element.kind, crate::types::internal::ElementKind::OcrText { .. }))
            .count(),
        output_paragraphs = result.len(),
        total_text_chars = result.iter().map(|paragraph| paragraph.text.len()).sum::<usize>(),
        "ocr_doc_to_paragraphs"
    );
}

#[cfg(feature = "ocr")]
fn make_ocr_block_paragraph(
    element: &crate::types::internal::InternalElement,
    page_height: f32,
) -> types::PdfParagraph {
    let block_bbox = pdf_block_bbox(element, page_height);
    let line_paragraphs = make_ocr_line_paragraphs(element, page_height);
    let lines = line_paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.lines.iter().cloned())
        .collect();
    make_ocr_paragraph(element.text.clone(), lines, block_bbox)
}

#[cfg(feature = "ocr")]
fn make_ocr_line_paragraphs(
    element: &crate::types::internal::InternalElement,
    page_height: f32,
) -> Vec<types::PdfParagraph> {
    let block_bbox = pdf_block_bbox(element, page_height);
    let text_lines = element.text.split('\n').collect::<Vec<_>>();
    let line_count = text_lines.len().max(1);

    text_lines
        .into_iter()
        .enumerate()
        .map(|(line_index, text)| make_ocr_line_paragraph(text, line_index, line_count, block_bbox))
        .collect()
}

#[cfg(feature = "ocr")]
fn pdf_block_bbox(element: &crate::types::internal::InternalElement, page_height: f32) -> Option<(f32, f32, f32, f32)> {
    element.bbox.as_ref().map(|bbox| {
        (
            bbox.x0 as f32,
            page_height - bbox.y1 as f32,
            bbox.x1 as f32,
            page_height - bbox.y0 as f32,
        )
    })
}

#[cfg(feature = "ocr")]
fn make_ocr_line_paragraph(
    text: &str,
    line_index: usize,
    line_count: usize,
    block_bbox: Option<(f32, f32, f32, f32)>,
) -> types::PdfParagraph {
    const DEFAULT_FONT_SIZE: f32 = 12.0;
    const DEFAULT_LINE_WIDTH: f32 = 100.0;

    let line_height = block_bbox
        .map(|(_, bottom, _, top)| (top - bottom) / line_count as f32)
        .unwrap_or(DEFAULT_FONT_SIZE);
    let line_bbox = block_bbox.map(|(left, _bottom, right, top)| {
        let line_top = top - line_index as f32 * line_height;
        (left, line_top - line_height, right, line_top)
    });
    let (x, baseline_y, width) = line_bbox
        .map(|(left, bottom, right, _)| (left, bottom, right - left))
        .unwrap_or((0.0, 0.0, DEFAULT_LINE_WIDTH));
    let lines = if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![make_ocr_pdf_line(
            text,
            x,
            baseline_y,
            width,
            line_height,
            DEFAULT_FONT_SIZE,
        )]
    };
    make_ocr_paragraph(text.to_string(), lines, line_bbox)
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn compatible_hint_indices(
    lines: &[types::PdfParagraph],
    hints: &[types::LayoutHint],
    selected: Vec<Option<usize>>,
    min_containment: f32,
) -> Vec<Option<usize>> {
    let mut compatible = vec![None; lines.len()];
    let mut previous_list_hint = None;
    for (index, line) in lines.iter().enumerate() {
        let actual = selected[index].filter(|&hint_index| {
            hints
                .get(hint_index)
                .is_some_and(|hint| classification_matches_hint(line, hint.class_name))
        });
        compatible[index] = actual
            .or_else(|| inherit_list_continuation(line, selected[index], previous_list_hint, hints, min_containment));
        previous_list_hint = compatible[index].filter(|&hint_index| {
            hints[hint_index].class_name == types::LayoutHintClass::ListItem && !line.text.trim().is_empty()
        });
    }
    compatible
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn classification_matches_hint(paragraph: &types::PdfParagraph, class_name: types::LayoutHintClass) -> bool {
    use types::LayoutHintClass as L;
    if paragraph.layout_class != Some(class_name) {
        return false;
    }
    match class_name {
        L::Title | L::SectionHeader => paragraph.heading_level.is_some(),
        L::Code => paragraph.is_code_block,
        L::Formula => paragraph.is_formula,
        L::ListItem => paragraph.is_list_item,
        L::Caption | L::Footnote => true,
        L::PageHeader | L::PageFooter | L::Picture => paragraph.is_page_furniture,
        _ => false,
    }
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn inherit_list_continuation(
    paragraph: &types::PdfParagraph,
    selected_hint: Option<usize>,
    previous_list_hint: Option<usize>,
    hints: &[types::LayoutHint],
    min_containment: f32,
) -> Option<usize> {
    let hint_index = previous_list_hint?;
    let hint = hints.get(hint_index)?;
    (selected_hint.is_none()
        && !paragraph.text.trim().is_empty()
        && hint_containment(paragraph.block_bbox?, hint) >= min_containment)
        .then_some(hint_index)
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn hint_containment(bbox: (f32, f32, f32, f32), hint: &types::LayoutHint) -> f32 {
    let intersection_width = (bbox.2.min(hint.right) - bbox.0.max(hint.left)).max(0.0);
    let intersection_height = (bbox.3.min(hint.top) - bbox.1.max(hint.bottom)).max(0.0);
    let paragraph_area = (bbox.2 - bbox.0).max(0.0) * (bbox.3 - bbox.1).max(0.0);
    if paragraph_area > 0.0 {
        intersection_width * intersection_height / paragraph_area
    } else {
        0.0
    }
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn regroup_layout_lines(lines: Vec<types::PdfParagraph>, hint_indices: Vec<Option<usize>>) -> Vec<types::PdfParagraph> {
    let mut result = Vec::new();
    let mut body_lines = Vec::new();
    let mut groups = group_by_hint(lines, hint_indices);

    for group in groups.drain(..) {
        if group.iter().any(has_structural_override) {
            push_body_group(&mut result, std::mem::take(&mut body_lines));
            if let Some(paragraph) = merge_structural_group(group) {
                result.push(paragraph);
            }
        } else {
            body_lines.extend(group);
        }
    }
    push_body_group(&mut result, body_lines);
    result
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn group_by_hint(lines: Vec<types::PdfParagraph>, hint_indices: Vec<Option<usize>>) -> Vec<Vec<types::PdfParagraph>> {
    let mut groups: Vec<(Option<usize>, Vec<types::PdfParagraph>)> = Vec::new();
    for (line, hint_index) in lines.into_iter().zip(hint_indices) {
        if let Some((last_hint, group)) = groups.last_mut()
            && hint_index.is_some()
            && *last_hint == hint_index
        {
            group.push(line);
        } else {
            groups.push((hint_index, vec![line]));
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn has_structural_override(paragraph: &types::PdfParagraph) -> bool {
    !paragraph.text.trim().is_empty()
        && (paragraph.heading_level.is_some()
            || paragraph.is_list_item
            || paragraph.is_code_block
            || paragraph.is_formula
            || paragraph.is_page_furniture
            || matches!(
                paragraph.layout_class,
                Some(types::LayoutHintClass::Caption | types::LayoutHintClass::Footnote)
            ))
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn push_body_group(result: &mut Vec<types::PdfParagraph>, lines: Vec<types::PdfParagraph>) {
    let lines = trim_blank_boundaries(lines);
    if lines.is_empty() {
        return;
    }
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return;
    }
    let bbox = union_bboxes(&lines);
    let pdf_lines = lines.into_iter().flat_map(|line| line.lines).collect();
    result.push(make_ocr_paragraph(text, pdf_lines, bbox));
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn merge_structural_group(lines: Vec<types::PdfParagraph>) -> Option<types::PdfParagraph> {
    let lines = trim_blank_boundaries(lines);
    let template = lines.iter().find(|line| has_structural_override(line))?.clone();
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let bbox = union_bboxes(&lines);
    let pdf_lines = lines.into_iter().flat_map(|line| line.lines).collect::<Vec<_>>();
    let mut merged = template;
    merged.word_count = types::PdfParagraph::compute_word_count(&text, &pdf_lines);
    merged.text = text;
    merged.lines = pdf_lines;
    merged.block_bbox = bbox;
    Some(merged)
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn trim_blank_boundaries(mut lines: Vec<types::PdfParagraph>) -> Vec<types::PdfParagraph> {
    let first_content = lines
        .iter()
        .position(|line| !line.text.trim().is_empty())
        .unwrap_or(lines.len());
    let retained = lines[first_content..]
        .iter()
        .rposition(|line| !line.text.trim().is_empty())
        .map_or(0, |index| index + 1);
    lines.drain(..first_content);
    lines.truncate(retained);
    lines
}

#[cfg(all(feature = "ocr", feature = "layout-detection"))]
fn union_bboxes(lines: &[types::PdfParagraph]) -> Option<(f32, f32, f32, f32)> {
    lines
        .iter()
        .filter_map(|line| line.block_bbox)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

#[cfg(feature = "ocr")]
fn make_ocr_paragraph(
    text: String,
    lines: Vec<types::PdfLine>,
    block_bbox: Option<(f32, f32, f32, f32)>,
) -> types::PdfParagraph {
    const DEFAULT_FONT_SIZE: f32 = 12.0;
    types::PdfParagraph {
        word_count: types::PdfParagraph::compute_word_count(&text, &lines),
        text,
        lines,
        dominant_font_size: DEFAULT_FONT_SIZE,
        heading_level: None,
        is_bold: false,
        is_list_item: false,
        is_code_block: false,
        is_formula: false,
        is_page_furniture: false,
        layout_class: None,
        layout_region_path: None,
        caption_for: None,
        block_bbox,
    }
}

#[cfg(feature = "ocr")]
fn make_ocr_pdf_line(
    text: &str,
    x: f32,
    baseline_y: f32,
    width: f32,
    line_height: f32,
    font_size: f32,
) -> types::PdfLine {
    let segment = crate::pdf::hierarchy::SegmentData {
        text: text.to_string(),
        x,
        y: baseline_y,
        width,
        height: line_height,
        font_size,
        is_bold: false,
        is_italic: false,
        is_monospace: false,
        baseline_y,
        assigned_role: None,
    };
    types::PdfLine {
        segments: vec![segment],
        baseline_y,
        dominant_font_size: font_size,
        is_bold: false,
        is_monospace: false,
    }
}

#[cfg(all(feature = "ocr", test))]
mod tests {
    use super::*;
    use crate::types::extraction::BoundingBox;
    use crate::types::internal::{ElementKind, InternalDocument, InternalElement};
    use crate::types::ocr_elements::OcrElementLevel;

    #[test]
    fn test_ocr_doc_soft_wrapped_body_stays_one_paragraph() {
        let mut doc = InternalDocument::new("test");
        let mut elem = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Block,
            },
            "First soft-wrapped body line\ncontinues on the next visual line",
            0,
        );
        elem.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 10.0,
            x1: 200.0,
            y1: 50.0,
        });
        doc.push_element(elem);

        let paragraphs = ocr_doc_to_paragraphs(&doc, 1000);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(
            paragraphs[0].text,
            "First soft-wrapped body line\ncontinues on the next visual line"
        );
        assert_eq!(paragraphs[0].lines.len(), 2);
    }

    /// Test that OCR elements with mixed content and blank lines preserve all text.
    #[test]
    fn test_ocr_doc_preserves_mixed_content_with_blanks() {
        let mut doc = InternalDocument::new("test");
        let mut elem = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Line,
            },
            "line1\n\nline3",
            0,
        );
        elem.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 10.0,
            x1: 100.0,
            y1: 70.0,
        });
        doc.push_element(elem);

        let paragraphs = ocr_doc_to_paragraphs(&doc, 1000);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "line1\n\nline3");
        assert_eq!(paragraphs[0].word_count, 2);
        assert_eq!(paragraphs[0].lines.len(), 2);
    }

    /// Test that whitespace-only OCR elements are filtered out (correct behavior).
    #[test]
    fn test_ocr_doc_filters_whitespace_only_elements() {
        let mut doc = InternalDocument::new("test");
        let mut elem1 = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Line,
            },
            "   \n  \n  ",
            0,
        );
        elem1.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 10.0,
            x1: 100.0,
            y1: 70.0,
        });
        doc.push_element(elem1);

        let mut elem2 = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Line,
            },
            "real content",
            0,
        );
        elem2.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 80.0,
            x1: 100.0,
            y1: 140.0,
        });
        doc.push_element(elem2);

        let paragraphs = ocr_doc_to_paragraphs(&doc, 1000);

        assert_eq!(paragraphs.len(), 1, "Should filter out whitespace-only element");
        assert_eq!(paragraphs[0].text, "real content");
    }

    /// Test that whitespace-only lines and their exact text remain in the OCR block.
    #[test]
    fn test_ocr_doc_whitespace_lines_text_preserved() {
        let mut doc = InternalDocument::new("test");
        let mut elem = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Line,
            },
            "Para1\n   \nPara2",
            0,
        );
        elem.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 10.0,
            x1: 100.0,
            y1: 70.0,
        });
        doc.push_element(elem);

        let paragraphs = ocr_doc_to_paragraphs(&doc, 1000);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "Para1\n   \nPara2");
        assert_eq!(paragraphs[0].lines.len(), 2);
        let line_height = (70.0 - 10.0) / 3.0;
        let para_1_y = 1000.0 - 10.0 - line_height;
        let para_2_y = 1000.0 - 10.0 - 3.0 * line_height;
        assert!(
            (paragraphs[0].lines[0].baseline_y - para_1_y).abs() < 0.1,
            "Line 1 Y position incorrect"
        );
        assert!(
            (paragraphs[0].lines[1].baseline_y - para_2_y).abs() < 0.1,
            "Line 2 Y position incorrect"
        );
    }

    /// Test that blank lines in OCR elements don't affect vertical positioning.
    /// When text contains blank lines (e.g., "A\n\nC"), the lines array should still
    /// have correct y-positions (0 for A, 2*line_height for C, not 1*line_height).
    /// This ensures correct sorting order when multiple paragraphs are interleaved.
    #[test]
    fn test_ocr_doc_blank_lines_preserve_vertical_spacing() {
        let mut doc = InternalDocument::new("test");
        let mut elem = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Line,
            },
            "Line1\n\nLine3",
            0,
        );
        elem.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 10.0,
            x1: 100.0,
            y1: 90.0,
        });
        doc.push_element(elem);

        let paragraphs = ocr_doc_to_paragraphs(&doc, 1000);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "Line1\n\nLine3");
        assert_eq!(paragraphs[0].lines.len(), 2);

        let expected_line_height = 80.0 / 3.0;

        assert!(
            (paragraphs[0].lines[0].baseline_y - (990.0 - expected_line_height)).abs() < 0.1,
            "Line1 baseline is incorrect: {}",
            paragraphs[0].lines[0].baseline_y
        );
        let first_segment = &paragraphs[0].lines[0].segments[0];
        assert!((first_segment.y + first_segment.height - 990.0).abs() < 0.1);
        assert!(
            (paragraphs[0].lines[1].baseline_y - (990.0 - 3.0 * expected_line_height)).abs() < 0.1,
            "Line3 baseline is incorrect: {}",
            paragraphs[0].lines[1].baseline_y
        );
    }

    /// Test that OCR elements with content followed by blanks preserve content.
    #[test]
    fn test_ocr_doc_preserves_content_before_blanks() {
        let mut doc = InternalDocument::new("test");
        let mut elem = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Line,
            },
            "important\n\n",
            0,
        );
        elem.bbox = Some(BoundingBox {
            x0: 10.0,
            y0: 10.0,
            x1: 100.0,
            y1: 70.0,
        });
        doc.push_element(elem);

        let paragraphs = ocr_doc_to_paragraphs(&doc, 1000);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "important\n\n");
        assert_eq!(paragraphs[0].word_count, 1);
        assert_eq!(
            paragraphs[0].lines.len(),
            1,
            "Only the non-blank line should be in lines array"
        );
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_multiline_ocr_block_applies_line_sized_layout_hint_only_to_matching_line() {
        use crate::pdf::structure::types::{LayoutHint, LayoutHintClass};

        let mut doc = InternalDocument::new("test");
        let mut elem = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Block,
            },
            "Document title\nFirst body line\nSecond body line",
            0,
        );
        elem.bbox = Some(BoundingBox {
            x0: 100.0,
            y0: 100.0,
            x1: 500.0,
            y1: 160.0,
        });
        doc.push_element(elem);

        let hints = [LayoutHint {
            class_name: LayoutHintClass::Title,
            confidence: 0.95,
            left: 100.0,
            bottom: 880.0,
            right: 500.0,
            top: 900.0,
        }];
        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "Document title");
        assert_eq!(paragraphs[1].text, "First body line\nSecond body line");
        assert_eq!(paragraphs[0].block_bbox, Some((100.0, 880.0, 500.0, 900.0)));
        assert_eq!(paragraphs[1].block_bbox, Some((100.0, 840.0, 500.0, 880.0)));
        assert_eq!(paragraphs[0].heading_level, Some(1));
        assert_eq!(paragraphs[1].heading_level, None);

        let assembled = crate::pdf::structure::assemble_internal_document(vec![paragraphs], &[], None, &[]);
        assert_eq!(assembled.elements.len(), 2);
        assert_eq!(assembled.elements[0].kind, ElementKind::Heading { level: 1 });
        assert_eq!(assembled.elements[1].kind, ElementKind::Paragraph);
        assert_eq!(assembled.elements[1].text, "First body line\nSecond body line");
    }

    #[cfg(feature = "layout-detection")]
    fn layout_test_document(text: &str, line_count: u32) -> InternalDocument {
        let mut doc = InternalDocument::new("test");
        let mut element = InternalElement::text(
            ElementKind::OcrText {
                level: OcrElementLevel::Block,
            },
            text,
            0,
        );
        element.bbox = Some(BoundingBox {
            x0: 100.0,
            y0: 100.0,
            x1: 500.0,
            y1: 100.0 + f64::from(line_count * 20),
        });
        doc.push_element(element);
        doc
    }

    #[cfg(feature = "layout-detection")]
    fn layout_test_hint(class_name: types::LayoutHintClass, bottom: f32, top: f32) -> types::LayoutHint {
        types::LayoutHint {
            class_name,
            confidence: 0.95,
            left: 100.0,
            bottom,
            right: 500.0,
            top,
        }
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_multiline_title_lines_under_same_hint_merge() {
        let doc = layout_test_document("A long document\nsubtitle line\nBody text", 3);
        let hints = [layout_test_hint(types::LayoutHintClass::Title, 860.0, 900.0)];

        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "A long document\nsubtitle line");
        assert_eq!(paragraphs[0].heading_level, Some(1));
        assert_eq!(paragraphs[0].block_bbox, Some((100.0, 860.0, 500.0, 900.0)));
        assert_eq!(paragraphs[1].text, "Body text");
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_multiline_code_lines_under_same_hint_merge() {
        let doc = layout_test_document("fn main() {\nprintln!(\"hello\");\nBody text", 3);
        let hints = [layout_test_hint(types::LayoutHintClass::Code, 860.0, 900.0)];

        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "fn main() {\nprintln!(\"hello\");");
        assert!(paragraphs[0].is_code_block);
        assert_eq!(paragraphs[1].text, "Body text");
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_rejected_title_prose_does_not_merge_with_title() {
        let prose = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen \
                     sixteen seventeen eighteen nineteen twenty twentyone twentytwo twentythree twentyfour twentyfive";
        let doc = layout_test_document(&format!("Document title\n{prose}"), 2);
        let hints = [layout_test_hint(types::LayoutHintClass::Title, 860.0, 900.0)];

        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "Document title");
        assert_eq!(paragraphs[0].heading_level, Some(1));
        assert_eq!(paragraphs[1].text, prose);
        assert_eq!(paragraphs[1].heading_level, None);
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_rejected_code_prose_does_not_merge_with_code() {
        let prose = "This ordinary prose sentence contains many words, and it clearly should remain body text rather than code.";
        let doc = layout_test_document(&format!("fn main() {{\n{prose}"), 2);
        let hints = [layout_test_hint(types::LayoutHintClass::Code, 860.0, 900.0)];

        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "fn main() {");
        assert!(paragraphs[0].is_code_block);
        assert_eq!(paragraphs[1].text, prose);
        assert!(!paragraphs[1].is_code_block);
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_wrapped_list_item_lines_under_same_hint_merge() {
        let doc = layout_test_document(
            "1. Wrapped item starts\nand continues here\nacross another line\nBody text",
            4,
        );
        let hints = [layout_test_hint(types::LayoutHintClass::ListItem, 840.0, 900.0)];

        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(
            paragraphs[0].text,
            "1. Wrapped item starts\nand continues here\nacross another line"
        );
        assert!(paragraphs[0].is_list_item);
        assert_eq!(paragraphs[1].text, "Body text");
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_title_split_trims_boundary_blank_before_assembled_body() {
        let doc = layout_test_document("Document title\n\nBody text", 3);
        let hints = [layout_test_hint(types::LayoutHintClass::Title, 880.0, 900.0)];
        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "Document title");
        assert_eq!(paragraphs[1].text, "Body text");

        let assembled = crate::pdf::structure::assemble_internal_document(vec![paragraphs], &[], None, &[]);
        assert_eq!(assembled.elements.len(), 2);
        assert_eq!(assembled.elements[0].kind, ElementKind::Heading { level: 1 });
        assert_eq!(assembled.elements[1].kind, ElementKind::Paragraph);
        assert_eq!(assembled.elements[1].text, "Body text");
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_unassociated_structural_line_does_not_capture_adjacent_body_lines() {
        let doc = layout_test_document("Promoted line\nBody one\nBody two", 3);
        let mut lines = make_ocr_line_paragraphs(&doc.elements[0], 1000.0);
        lines[0].heading_level = Some(1);

        let paragraphs = regroup_layout_lines(lines, vec![None, None, None]);

        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "Promoted line");
        assert_eq!(paragraphs[0].heading_level, Some(1));
        assert_eq!(paragraphs[1].text, "Body one\nBody two");
        assert_eq!(paragraphs[1].heading_level, None);
    }

    #[cfg(feature = "layout-detection")]
    #[test]
    fn test_picture_uses_canonical_match_identity() {
        let doc = layout_test_document("Detected picture region", 1);
        let hints = [layout_test_hint(types::LayoutHintClass::Picture, 880.0, 900.0)];

        let paragraphs = ocr_doc_to_layout_paragraphs(&doc, 1000, &hints, 0.5, 0.2);

        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].layout_class, Some(types::LayoutHintClass::Picture));
        assert!(paragraphs[0].is_page_furniture);
    }
}
