//! PDF text extraction using the pdf_oxide backend.

use super::OxideDocument;
use crate::core::config::{ExtractionConfig, PageConfig};
use crate::pdf::error::{PdfError, Result};
use crate::pdf::metadata::PdfExtractionMetadata;
use crate::pdf::structure::constants::{COALESCE_THRESHOLD, MAX_GLYPH_JITTER_PT, MIN_DISORDER_COUNT};
use crate::pdf::text::{contains_html_markup, fix_pdf_control_chars};
use crate::types::{PageBoundary, PageContent};
use pdf_oxide::document::ReadingOrder;
use std::borrow::Cow;

/// Result type for PDF text extraction with optional page tracking.
type PdfTextExtractionResult = (String, Option<Vec<PageBoundary>>, Option<Vec<PageContent>>);

/// Result type for unified PDF text and metadata extraction.
///
/// Contains text, optional page boundaries, optional per-page content, and metadata.
pub type OxideUnifiedExtractionResult = (
    String,
    Option<Vec<PageBoundary>>,
    Option<Vec<PageContent>>,
    PdfExtractionMetadata,
);

/// Extract text and metadata from a PDF document in a single pass.
///
/// This is the oxide equivalent of `extract_text_and_metadata_from_pdf_document`.
/// It extracts both text and metadata in one pass through the document.
pub(crate) fn extract_text_and_metadata(
    doc: &mut OxideDocument,
    extraction_config: Option<&ExtractionConfig>,
) -> Result<OxideUnifiedExtractionResult> {
    let page_config = extraction_config.and_then(|c| c.pages.as_ref());
    let (text, boundaries, page_contents) = extract_text_from_oxide_document(doc, page_config, extraction_config)?;

    let scanned_min_confidence = extraction_config
        .map(|c| c.ocr_strategy.effective_min_confidence())
        .unwrap_or(crate::core::config::DEFAULT_SCANNED_MIN_CONFIDENCE);
    let ocr_quality_thresholds = extraction_config
        .and_then(|c| c.ocr.as_ref())
        .and_then(|o| o.quality_thresholds.clone())
        .unwrap_or_default();
    let metadata = super::metadata::extract_metadata_from_oxide_document(
        doc,
        boundaries.as_deref(),
        &text,
        scanned_min_confidence,
        &ocr_quality_thresholds,
    )?;

    Ok((text, boundaries, page_contents, metadata))
}

/// Extract text spans with bounding boxes from a single page.
///
/// Returns `(text_spans)` where each span contains the text, x, y, width, and height
/// in PDF coordinate space (points, y=0 at bottom of page).
///
/// This is used by reading-order reconstruction to project spans onto layout regions.
#[cfg(feature = "layout-detection")]
pub(crate) fn extract_spans_from_page(
    doc: &mut pdf_oxide::PdfDocument,
    page_index: usize,
) -> Result<(Vec<crate::extractors::pdf::reading_order::TextSpan>, bool)> {
    use pdf_oxide::document::ReadingOrder;

    let mut page_text_data = super::guard_oxide_panic(
        || {
            doc.extract_page_text_with_options(page_index, ReadingOrder::ColumnAware)
                .map_err(|e| PdfError::TextExtractionFailed(format!("Failed to extract page text: {}", e)))
        },
        |panic| PdfError::TextExtractionFailed(format!("Page text extraction panicked in pdf_oxide: {}", panic)),
    )?;
    let reordered_sparse_columns = reorder_sparse_two_column_page(&mut page_text_data.spans, page_text_data.page_width);

    let spans = page_text_data
        .spans
        .iter()
        .map(|span| crate::extractors::pdf::reading_order::TextSpan {
            text: span.text.clone(),
            x: span.bbox.x,
            y: span.bbox.y,
            width: span.bbox.width,
            height: span.bbox.height,
        })
        .collect();

    Ok((spans, reordered_sparse_columns))
}

/// Extract text from a pdf_oxide document with optional page boundary tracking.
///
/// Mirrors the signature and behaviour of `extract_text_from_pdf_document`.
///
/// When `page_config` is `Some`, tracks byte offsets and optionally collects
/// per-page `PageContent` entries.
///
/// When `page_config` is `None` but `extraction_config` requires per-page boundaries
/// (i.e. `force_ocr_pages` is set or an `ocr` config is present for quality evaluation),
/// boundary tracking is enabled automatically with a default `PageConfig` so that the
/// mixed-OCR and quality-threshold codepaths receive the offsets they need.
///
/// Otherwise the fast path is used (no per-page tracking).
pub(crate) fn extract_text_from_oxide_document(
    doc: &mut OxideDocument,
    page_config: Option<&PageConfig>,
    extraction_config: Option<&ExtractionConfig>,
) -> Result<PdfTextExtractionResult> {
    let needs_boundaries =
        extraction_config.is_some_and(|c| c.force_ocr_pages.as_ref().is_some_and(|p| !p.is_empty()) || c.ocr.is_some());

    if let Some(config) = page_config {
        extract_text_with_tracking(doc, config)
    } else if needs_boundaries {
        let default_config = PageConfig::default();
        extract_text_with_tracking(doc, &default_config)
    } else {
        extract_text_fast_path(doc)
    }
}

/// Fast path: extract text without page tracking.
///
/// Iterates pages one-by-one, applies control-char fixes and optional HTML
/// conversion, and builds a single concatenated string. Pre-allocates capacity
/// after sampling the first 5 pages.
fn extract_text_fast_path(doc: &mut OxideDocument) -> Result<PdfTextExtractionResult> {
    let page_count = doc
        .doc
        .page_count()
        .map_err(|e| PdfError::TextExtractionFailed(format!("Failed to get page count: {}", e)))?;

    let mut content = String::new();
    let mut total_sample_size = 0usize;
    let mut sample_count = 0;

    for page_idx in 0..page_count {
        let page_text = extract_page_text_column_aware(&mut doc.doc, page_idx)?;

        let page_size = page_text.len();

        if page_idx > 0 {
            content.push_str("\n\n");
        }

        let cleaned = apply_text_cleanup(&page_text);
        content.push_str(&cleaned);

        if page_idx < 5 {
            total_sample_size += page_size;
            sample_count += 1;
        }

        if page_idx == 4 && sample_count > 0 && page_count > 5 {
            let avg_page_size = total_sample_size / sample_count;
            let estimated_remaining = avg_page_size * (page_count - 5);
            content.reserve(estimated_remaining + (estimated_remaining / 10));
        }
    }

    Ok((content, None, None))
}

/// Extract text with page boundary and content tracking.
///
/// Mirrors `extract_text_lazy_with_tracking`: tracks byte
/// offsets for each page, optionally collects per-page `PageContent`, and inserts
/// page markers when configured.
fn extract_text_with_tracking(doc: &mut OxideDocument, config: &PageConfig) -> Result<PdfTextExtractionResult> {
    let page_count = doc
        .doc
        .page_count()
        .map_err(|e| PdfError::TextExtractionFailed(format!("Failed to get page count: {}", e)))?;

    let mut content = String::new();
    let mut boundaries = Vec::with_capacity(page_count);
    let mut page_contents = if config.extract_pages {
        Some(Vec::with_capacity(page_count))
    } else {
        None
    };

    let mut total_sample_size = 0usize;
    let mut sample_count = 0;

    for page_idx in 0..page_count {
        let page_number = page_idx + 1;

        let page_text = extract_page_text_column_aware(&mut doc.doc, page_idx)?;

        let page_size = page_text.len();

        if page_idx < 5 {
            total_sample_size += page_size;
            sample_count += 1;
        }

        if config.insert_page_markers {
            let marker = config.marker_format.replace("{page_num}", &page_number.to_string());
            content.push_str(&marker);
        } else if page_idx > 0 {
            content.push_str("\n\n");
        }

        let cleaned = apply_text_cleanup(&page_text);

        let byte_start = content.len();
        content.push_str(&cleaned);
        let byte_end = content.len();

        boundaries.push(PageBoundary {
            byte_start,
            byte_end,
            page_number: page_number as u32,
        });

        if let Some(ref mut pages) = page_contents {
            let is_blank = Some(crate::extraction::blank_detection::is_page_text_blank(&cleaned));
            pages.push(PageContent {
                page_number: page_number as u32,
                content: cleaned.into_owned(),
                tables: Vec::new(),
                image_indices: Vec::new(),
                hierarchy: None,
                is_blank,
                layout_regions: None,
                speaker_notes: None,
                section_name: None,
                sheet_name: None,
            });
        }

        if page_idx == 4 && page_count > 5 && sample_count > 0 {
            let avg_page_size = total_sample_size / sample_count;
            let estimated_remaining = avg_page_size * (page_count - 5);
            let separator_overhead = (page_count - 5) * 3;
            content.reserve(estimated_remaining + separator_overhead + (estimated_remaining / 10));
        }
    }

    Ok((content, Some(boundaries), page_contents))
}

/// Collect Widget annotation field values for the given page, sorted top-to-bottom.
///
/// Returns `(mid_y_pdf, value_text)` pairs. `mid_y_pdf` is the vertical midpoint of
/// the Widget's bounding rectangle in PDF page coordinates (Y=0 at bottom of page,
/// higher values are higher on the page). The list is sorted descending by Y so that
/// entries nearer the top of the page come first, preserving visual reading order when
/// the values are appended to the assembled span text.
///
/// Empty values and annotations without a `/V` entry are excluded. This function is
/// intentionally infallible: a failed `get_annotations` call is logged at DEBUG level
/// and returns an empty list so that the rest of the extraction path is unaffected.
fn collect_widget_field_values(doc: &pdf_oxide::PdfDocument, page_index: usize) -> Vec<(f64, String)> {
    let annotations = match doc.get_annotations(page_index) {
        Ok(a) => a,
        Err(e) => {
            tracing::debug!(
                page = page_index,
                "pdf_oxide: could not read annotations for widget values: {e}"
            );
            return Vec::new();
        }
    };

    let mut widgets: Vec<(f64, String)> = annotations
        .into_iter()
        .filter(|a| a.subtype_enum == pdf_oxide::AnnotationSubtype::Widget)
        .filter_map(|a| {
            let value = a.field_value?.trim().to_string();
            if value.is_empty() {
                return None;
            }
            let mid_y = a.rect.map_or(f64::NEG_INFINITY, |r| (r[1] + r[3]) / 2.0);
            Some((mid_y, value))
        })
        .collect();

    widgets.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    widgets
}

/// Append Widget form-field values that are absent from `text`.
///
/// Handles interactive (non-flattened) PDFs where field values live only in Widget `/V`
/// entries and are absent from the page content stream. Values already present in `text`
/// (e.g. flattened PDFs where the appearance stream was rendered into the content stream)
/// are skipped to prevent duplication.
///
/// Deduplication uses substring matching: if `value` appears anywhere in `text` the field
/// is skipped. This is intentionally simple — the common case is a verbatim match between
/// the rendered appearance text and the Widget `/V` string. It can produce false negatives
/// when the field value is a substring of surrounding prose (e.g. value "Smith" suppressed
/// when content already contains "John Smith"). This is an acceptable trade-off to avoid
/// duplicating values in flattened PDFs; tighter word-boundary deduplication can be added
/// when evidence of real-world false negatives is available.
///
/// Values are appended after all content-stream text, not interleaved at their bounding-box
/// positions. This is the intended ordering for the initial implementation: interactive
/// PDFs rarely have dense label+value proximity requirements, and span-level interleaving
/// would require re-sorting the column-aware span list which is not guaranteed to be
/// monotonically ordered by Y.
///
/// Appends in top-to-bottom page order (descending by annotation mid-Y).
fn append_missing_widget_values(text: &mut String, widgets: &[(f64, String)]) {
    for (_, value) in widgets {
        if !text.contains(value.as_str()) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(value);
        }
    }
}

/// Returns true when `spans` exhibits the glyph-fragmentation signature (issue #962).
///
/// See `crate::pdf::structure::constants` for the threshold values and their justification.
///
/// pdf_oxide's ColumnAware reading order groups all spans at one y-level before moving
/// to the next. For Word-exported PDFs where each glyph has its own BT…ET block with a
/// sinusoidal y-jitter, this produces groups ordered by y-level rather than by reading
/// order: "et" (y=703) appears before "H" (y=700) even though "H" comes first visually.
///
/// Two-part signature:
/// 1. Both spans are short (≤ 3 chars): per-glyph BT/ET always produces single-character
///    spans; multi-character spans are word-level and cannot be glyph artifacts.
/// 2. The spans are on the same visual line (y-gap ≤ MAX_GLYPH_JITTER_PT when heights
///    are zero, or < half the measured height otherwise) yet the x-coordinate resets
///    significantly leftward — indicating a new y-group started mid-reading-order.
///
/// ≥ MIN_DISORDER_COUNT such events means position-based reconstruction is needed.
fn is_fragmented_span_list(spans: &[pdf_oxide::layout::TextSpan]) -> bool {
    let mut disorder_count = 0;
    for window in spans.windows(2) {
        let prev = &window[0];
        let cur = &window[1];

        if prev.text.chars().count() > 3 || cur.text.chars().count() > 3 {
            continue;
        }

        let y_gap = (prev.bbox.y - cur.bbox.y).abs();

        let eff_height = prev.bbox.height.max(cur.bbox.height);
        let same_line = if eff_height > 0.0 {
            y_gap < eff_height * 0.5
        } else {
            y_gap <= MAX_GLYPH_JITTER_PT
        };

        if same_line && cur.bbox.x < prev.bbox.x - prev.font_size {
            disorder_count += 1;
            if disorder_count >= MIN_DISORDER_COUNT {
                return true;
            }
        }
    }
    false
}

/// Rebuild readable text from a glyph-fragmented span list (issue #962).
///
/// Algorithm:
/// 1. Sort spans by y-descending (top-of-page first in PDF coordinates).
/// 2. Group by chained y-proximity: consecutive spans within COALESCE_THRESHOLD pt
///    of the previous span belong to the same visual line.
/// 3. Within each group sort by x-ascending (left-to-right reading order).
/// 4. Concatenate, inserting a space wherever the x-gap between adjacent spans
///    exceeds font_size * 0.5.
fn rebuild_text_from_fragmented_spans(spans: &[pdf_oxide::layout::TextSpan]) -> String {
    if spans.is_empty() {
        return String::new();
    }

    let mut sorted: Vec<&pdf_oxide::layout::TextSpan> = spans.iter().collect();
    sorted.sort_by(|a, b| b.bbox.y.partial_cmp(&a.bbox.y).unwrap_or(std::cmp::Ordering::Equal));

    let mut groups: Vec<Vec<&pdf_oxide::layout::TextSpan>> = Vec::new();
    for span in sorted {
        let belongs = groups.last().is_some_and(|g| {
            let prev_y = g.last().unwrap().bbox.y;
            (span.bbox.y - prev_y).abs() <= COALESCE_THRESHOLD
        });
        if belongs {
            groups.last_mut().unwrap().push(span);
        } else {
            groups.push(vec![span]);
        }
    }

    let mut result = String::new();
    for (gi, group) in groups.iter_mut().enumerate() {
        group.sort_by(|a, b| a.bbox.x.partial_cmp(&b.bbox.x).unwrap_or(std::cmp::Ordering::Equal));
        if gi > 0 {
            result.push('\n');
        }
        let font_size = group.iter().map(|s| s.font_size).fold(0.0_f32, f32::max);
        let space_threshold = font_size * 0.5;
        let mut prev_end_x = f32::NEG_INFINITY;
        for span in group.iter() {
            if prev_end_x.is_finite() && span.bbox.x - prev_end_x > space_threshold {
                result.push(' ');
            }
            result.push_str(&span.text);
            prev_end_x = span.bbox.x + span.bbox.width;
        }
    }
    result
}

const INLINE_FRAGMENT_GAP_RATIO: f32 = 0.1;
// Detached glyphs are stream-local; bounding the lookup avoids quadratic work on dense pages.
const MAX_INLINE_FRAGMENT_ANCHOR_LOOKBACK: usize = 256;
const ROW_RESET_MIN_BACKTRACK_EMS: f32 = 4.0;

#[derive(Clone, Copy)]
struct OrderedSpan<'a> {
    span: &'a pdf_oxide::layout::TextSpan,
    glue_to_previous: bool,
}

fn spans_overlap_vertically(first: &pdf_oxide::layout::TextSpan, second: &pdf_oxide::layout::TextSpan) -> bool {
    let overlap_start = first.bbox.y.max(second.bbox.y);
    let overlap_end = (first.bbox.y + first.bbox.height).min(second.bbox.y + second.bbox.height);
    overlap_end > overlap_start
}

fn is_short_inline_fragment(span: &pdf_oxide::layout::TextSpan) -> bool {
    let mut chars = span.text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let char_count = 1 + chars.count();
    if char_count > 3 || span.text.chars().all(char::is_whitespace) {
        return false;
    }
    !(char_count == 1 && matches!(first, 'a' | 'A' | 'I'))
}

fn has_rtl_or_bidi_content(text: &str) -> bool {
    text.chars()
        .any(|character| pdf_oxide::text::is_rtl_text(character as u32))
}

fn is_horizontal_ltr(span: &pdf_oxide::layout::TextSpan) -> bool {
    span.wmode == 0 && !span.rtl_draw_logical && span.rotation_degrees.abs() <= f32::EPSILON
}

fn find_inline_fragment_anchor(
    index: usize,
    spans: &[pdf_oxide::layout::TextSpan],
    anchors: &[Option<usize>],
) -> Option<usize> {
    let span = &spans[index];
    if span.split_boundary_before
        || !is_short_inline_fragment(span)
        || !is_horizontal_ltr(span)
        || has_rtl_or_bidi_content(&span.text)
    {
        return None;
    }

    let search_start = index.saturating_sub(MAX_INLINE_FRAGMENT_ANCHOR_LOOKBACK);
    (search_start..index)
        .filter(|candidate_index| anchors[*candidate_index].is_none())
        .filter_map(|candidate_index| {
            let candidate = &spans[candidate_index];
            if !is_horizontal_ltr(candidate)
                || has_rtl_or_bidi_content(&candidate.text)
                || !spans_overlap_vertically(candidate, span)
                || (candidate.rotation_degrees - span.rotation_degrees).abs() > f32::EPSILON
            {
                return None;
            }
            let gap = span.bbox.x - (candidate.bbox.x + candidate.bbox.width);
            let tolerance = candidate.font_size.max(span.font_size) * INLINE_FRAGMENT_GAP_RATIO;
            (gap >= -tolerance && gap <= tolerance).then_some((candidate_index, gap.abs()))
        })
        .min_by(|(_, first_gap), (_, second_gap)| {
            first_gap.partial_cmp(second_gap).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(candidate_index, _)| candidate_index)
}

fn order_spans_with_inline_fragments(spans: &[pdf_oxide::layout::TextSpan]) -> Vec<OrderedSpan<'_>> {
    let mut anchors = vec![None; spans.len()];
    for index in 0..spans.len() {
        anchors[index] = find_inline_fragment_anchor(index, spans, &anchors);
    }

    let mut children = vec![Vec::new(); spans.len()];
    for (index, anchor) in anchors.iter().enumerate() {
        if let Some(anchor) = anchor {
            children[*anchor].push(index);
        }
    }
    for attached in &mut children {
        attached.sort_by(|first, second| {
            spans[*first]
                .bbox
                .x
                .partial_cmp(&spans[*second].bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let mut ordered = Vec::with_capacity(spans.len());
    for (index, span) in spans.iter().enumerate() {
        if anchors[index].is_some() {
            continue;
        }
        ordered.push(OrderedSpan {
            span,
            glue_to_previous: false,
        });
        ordered.extend(children[index].iter().map(|child| OrderedSpan {
            span: &spans[*child],
            glue_to_previous: true,
        }));
    }
    ordered
}

fn append_span_separator(
    text: &mut String,
    previous: &pdf_oxide::layout::TextSpan,
    current: OrderedSpan<'_>,
    paragraph_gap_threshold: f32,
    allow_ltr_row_resets: bool,
) {
    if current.glue_to_previous {
        return;
    }

    let span = current.span;
    let y_gap = (previous.bbox.y - span.bbox.y).abs();
    let reset_threshold = previous.font_size.max(span.font_size) * ROW_RESET_MIN_BACKTRACK_EMS;
    let is_horizontal_ltr_pair = is_horizontal_ltr(previous)
        && is_horizontal_ltr(span)
        && !has_rtl_or_bidi_content(&previous.text)
        && !has_rtl_or_bidi_content(&span.text);
    if allow_ltr_row_resets && is_horizontal_ltr_pair && span.bbox.x < previous.bbox.x - reset_threshold {
        if y_gap > paragraph_gap_threshold {
            text.push_str("\n\n");
        } else {
            text.push('\n');
        }
        return;
    }

    if span.split_boundary_before {
        if !previous.text.ends_with(char::is_whitespace) && !span.text.starts_with(char::is_whitespace) {
            text.push(' ');
        }
        return;
    }

    let previous_end_x = previous.bbox.x + previous.bbox.width;
    let effective_height = span.bbox.height.max(previous.bbox.height).max(span.font_size * 0.5);
    if y_gap < effective_height * 0.5 {
        if span.bbox.x - previous_end_x > span.font_size * 0.15 {
            text.push(' ');
        }
    } else if y_gap > paragraph_gap_threshold {
        text.push_str("\n\n");
    } else {
        text.push('\n');
    }
}

fn assemble_page_text(spans: &[pdf_oxide::layout::TextSpan]) -> String {
    let mut heights: Vec<f32> = spans.iter().map(|span| span.bbox.height).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_height = if heights.is_empty() {
        1.0
    } else {
        heights[heights.len() / 2]
    };
    let paragraph_gap_threshold = median_height * 1.5;

    tracing::debug!(
        span_count = spans.len(),
        median_height,
        paragraph_gap_threshold,
        "paragraph break detection initialized"
    );

    let ordered = order_spans_with_inline_fragments(spans);
    let allow_ltr_row_resets = !spans
        .iter()
        .any(|span| span.rtl_draw_logical || has_rtl_or_bidi_content(&span.text));
    let mut text = String::with_capacity(spans.len() * 20);
    let mut prev_span: Option<&pdf_oxide::layout::TextSpan> = None;

    for current in ordered {
        let span = current.span;
        if let Some(prev) = prev_span {
            append_span_separator(&mut text, prev, current, paragraph_gap_threshold, allow_ltr_row_resets);
        }
        text.push_str(&span.text);
        prev_span = Some(span);
    }

    text
}

// pdf_oxide's XY-Cut does not split regions with fewer than five spans.
// These guards cover the issue #1345 four-span sentence without reclassifying
// sparse tables or forms as prose columns.
const MIN_SPARSE_COLUMN_GUTTER_FRACTION: f32 = 0.05;
const MIN_SPARSE_COLUMN_GUTTER_PTS: f32 = 15.0;
const MIN_SPARSE_COLUMN_CONTENT_WIDTH_PTS: f32 = 144.0;
const MIN_SPARSE_COLUMN_WORDS: usize = 2;
const MIN_SPARSE_COLUMN_WORDS_PER_SIDE: usize = 6;
const MIN_SPARSE_COLUMN_ALPHA_CHARS: usize = 8;
const MIN_SPARSE_COLUMN_ALPHA_RATIO: f32 = 0.55;
const MIN_SPARSE_COLUMN_VERTICAL_OVERLAP: f32 = 0.5;
const XY_CUT_MIN_SPANS_FOR_SPLIT: usize = 5;

fn is_sparse_column_prose(span: &pdf_oxide::layout::TextSpan) -> bool {
    let alpha_chars = span.text.chars().filter(|character| character.is_alphabetic()).count();
    let non_whitespace_chars = span.text.chars().filter(|character| !character.is_whitespace()).count();
    let word_count = span.text.split_whitespace().count();
    let geometry_is_valid = span.bbox.x.is_finite()
        && span.bbox.y.is_finite()
        && span.bbox.width.is_finite()
        && span.bbox.height.is_finite()
        && span.bbox.width > 0.0;

    geometry_is_valid
        && !span.is_monospace
        && is_horizontal_ltr(span)
        && !has_rtl_or_bidi_content(&span.text)
        && !span.text.contains(':')
        && word_count >= MIN_SPARSE_COLUMN_WORDS
        && alpha_chars >= MIN_SPARSE_COLUMN_ALPHA_CHARS
        && alpha_chars as f32 / non_whitespace_chars.max(1) as f32 >= MIN_SPARSE_COLUMN_ALPHA_RATIO
}

fn sparse_columns_overlap(left: &[&pdf_oxide::layout::TextSpan], right: &[&pdf_oxide::layout::TextSpan]) -> bool {
    let extent = |side: &[&pdf_oxide::layout::TextSpan]| {
        side.iter()
            .map(|span| span.bbox.y)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), y| {
                (low.min(y), high.max(y))
            })
    };
    let (left_low, left_high) = extent(left);
    let (right_low, right_high) = extent(right);
    let overlap = (left_high.min(right_high) - left_low.max(right_low)).max(0.0);
    let shorter_extent = (left_high - left_low).min(right_high - right_low);

    shorter_extent > 0.0 && overlap / shorter_extent >= MIN_SPARSE_COLUMN_VERTICAL_OVERLAP
}

fn sparse_columns_continue_one_sentence(
    left: &[&pdf_oxide::layout::TextSpan],
    right: &[&pdf_oxide::layout::TextSpan],
) -> bool {
    let mut left_by_y = left.to_vec();
    let mut right_by_y = right.to_vec();
    left_by_y.sort_by(|first, second| second.bbox.y.total_cmp(&first.bbox.y));
    right_by_y.sort_by(|first, second| second.bbox.y.total_cmp(&first.bbox.y));
    let starts_lowercase = |span: &&pdf_oxide::layout::TextSpan| {
        span.text
            .chars()
            .find(|character| character.is_alphabetic())
            .is_some_and(char::is_lowercase)
    };
    let starts_uppercase = |span: &&pdf_oxide::layout::TextSpan| {
        span.text
            .chars()
            .find(|character| character.is_alphabetic())
            .is_some_and(char::is_uppercase)
    };
    let has_terminal = |span: &&pdf_oxide::layout::TextSpan| {
        span.text
            .trim_end()
            .ends_with(|character: char| matches!(character, '.' | '!' | '?'))
    };
    let continuations = [&left_by_y[1], &right_by_y[0], &right_by_y[1]];
    let all_spans = left_by_y.iter().chain(&right_by_y);

    starts_uppercase(&left_by_y[0])
        && continuations.into_iter().all(starts_lowercase)
        && all_spans.clone().filter(|span| has_terminal(span)).count() == 1
        && has_terminal(&right_by_y[1])
}

fn is_sparse_column_split(spans: &[pdf_oxide::layout::TextSpan], split_x: f32, min_gutter: f32) -> bool {
    let left: Vec<_> = spans.iter().filter(|span| span.bbox.x < split_x).collect();
    let right: Vec<_> = spans.iter().filter(|span| span.bbox.x >= split_x).collect();
    if left.len() != 2 || right.len() != 2 {
        return false;
    }
    let word_count = |side: &[&pdf_oxide::layout::TextSpan]| {
        side.iter()
            .map(|span| span.text.split_whitespace().count())
            .sum::<usize>()
    };
    if word_count(&left) < MIN_SPARSE_COLUMN_WORDS_PER_SIDE || word_count(&right) < MIN_SPARSE_COLUMN_WORDS_PER_SIDE {
        return false;
    }
    let left_right = left
        .iter()
        .map(|span| span.bbox.x + span.bbox.width)
        .fold(f32::NEG_INFINITY, f32::max);

    split_x - left_right >= min_gutter
        && sparse_columns_overlap(&left, &right)
        && sparse_columns_continue_one_sentence(&left, &right)
}

fn sparse_column_split(spans: &[pdf_oxide::layout::TextSpan], page_width: f32) -> Option<f32> {
    let has_sparse_prose_shape =
        spans.len() == XY_CUT_MIN_SPANS_FOR_SPLIT - 1 && spans.iter().all(is_sparse_column_prose);
    let content_left = spans.iter().map(|span| span.bbox.x).fold(f32::INFINITY, f32::min);
    let content_right = spans
        .iter()
        .map(|span| span.bbox.x + span.bbox.width)
        .fold(f32::NEG_INFINITY, f32::max);
    if !has_sparse_prose_shape || content_right - content_left < MIN_SPARSE_COLUMN_CONTENT_WIDTH_PTS {
        return None;
    }
    let min_gutter = (page_width * MIN_SPARSE_COLUMN_GUTTER_FRACTION).max(MIN_SPARSE_COLUMN_GUTTER_PTS);
    let mut starts: Vec<f32> = spans.iter().map(|span| span.bbox.x).collect();
    starts.sort_by(f32::total_cmp);
    starts.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);

    starts
        .into_iter()
        .find(|&split_x| is_sparse_column_split(spans, split_x, min_gutter))
}

/// Reorder the guarded four-span, two-column sentence shape.
///
/// Returns `true` only when the sparse prose classifier matched and reordered
/// the spans. Callers use this signal to preserve the result across a broad
/// single layout hint.
pub(crate) fn reorder_sparse_two_column_page(spans: &mut [pdf_oxide::layout::TextSpan], page_width: f32) -> bool {
    let Some(split_x) = sparse_column_split(spans, page_width) else {
        return false;
    };
    spans.sort_by(|left, right| {
        let left_column = usize::from(left.bbox.x >= split_x);
        let right_column = usize::from(right.bbox.x >= split_x);
        left_column
            .cmp(&right_column)
            .then_with(|| right.bbox.y.total_cmp(&left.bbox.y))
            .then_with(|| left.bbox.x.total_cmp(&right.bbox.x))
    });
    true
}

/// Extract text from one page with column-aware ordering and guarded repairs.
///
/// Applies sparse-column and glyph-fragmentation repairs before assembling the
/// page text.
fn extract_page_text_column_aware(doc: &mut pdf_oxide::PdfDocument, page_index: usize) -> Result<String> {
    let widgets = collect_widget_field_values(doc, page_index);

    let mut page_text_data = super::guard_oxide_panic(
        || {
            doc.extract_page_text_with_options(page_index, ReadingOrder::ColumnAware)
                .map_err(|e| {
                    PdfError::TextExtractionFailed(format!("Page {} text extraction failed: {}", page_index + 1, e))
                })
        },
        |panic| {
            PdfError::TextExtractionFailed(format!(
                "Page {} text extraction panicked in pdf_oxide: {}",
                page_index + 1,
                panic
            ))
        },
    )?;

    reorder_sparse_two_column_page(&mut page_text_data.spans, page_text_data.page_width);

    if is_fragmented_span_list(&page_text_data.spans) {
        tracing::debug!(
            span_count = page_text_data.spans.len(),
            "glyph fragmentation detected — rebuilding text from span positions (#962)"
        );
        let mut text = rebuild_text_from_fragmented_spans(&page_text_data.spans);
        append_missing_widget_values(&mut text, &widgets);
        return Ok(text);
    }

    let mut text = assemble_page_text(&page_text_data.spans);

    append_missing_widget_values(&mut text, &widgets);

    Ok(text)
}

/// Apply common text cleanup: fix control chars and optionally convert HTML.
///
/// Returns a `Cow` to avoid allocation when the text is already clean.
fn apply_text_cleanup(text: &str) -> Cow<'_, str> {
    let cleaned = fix_pdf_control_chars(text);

    #[cfg(feature = "html")]
    if contains_html_markup(&cleaned) {
        return Cow::Owned(crate::pdf::text::convert_html_page_text(&cleaned));
    }

    #[cfg(not(feature = "html"))]
    let _ = contains_html_markup(&cleaned);

    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_oxide::geometry::Rect;
    use pdf_oxide::layout::TextSpan;

    fn span(text: &str, x: f32, y: f32, height: f32, font_size: f32) -> TextSpan {
        span_with_width(text, x, y, font_size * 0.6, height, font_size)
    }

    fn span_with_width(text: &str, x: f32, y: f32, width: f32, height: f32, font_size: f32) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            bbox: Rect { x, y, width, height },
            font_size,
            ..TextSpan::default()
        }
    }

    /// Build a list of N single-char spans that each trigger a same-line x-disorder
    /// event. All at the same y (zero height fallback path), each span's x is
    /// `prev.x - font_size - 1` so cur.x < prev.x - font_size is always true.
    fn disorder_spans(count: usize) -> Vec<TextSpan> {
        let font_size = 12.0_f32;
        let mut spans = Vec::with_capacity(count + 1);
        let mut x = 300.0_f32;
        for _i in 0..=count {
            spans.push(span("A", x, 700.0, 0.0, font_size));
            x = x - font_size - 1.0;
        }
        spans
    }

    #[test]
    fn fragmentation_detected_at_threshold() {
        let spans = disorder_spans(MIN_DISORDER_COUNT);
        assert!(
            is_fragmented_span_list(&spans),
            "should detect fragmentation at exactly MIN_DISORDER_COUNT ({MIN_DISORDER_COUNT}) events"
        );
    }

    #[test]
    fn fragmentation_not_detected_below_threshold() {
        let spans = disorder_spans(MIN_DISORDER_COUNT - 1);
        assert!(
            !is_fragmented_span_list(&spans),
            "must NOT detect fragmentation with {} events (threshold is {MIN_DISORDER_COUNT})",
            MIN_DISORDER_COUNT - 1
        );
    }

    #[test]
    fn long_spans_never_count_toward_disorder() {
        let font_size = 12.0_f32;
        let mut spans = Vec::new();
        let mut x = 500.0_f32;
        for _ in 0..20 {
            spans.push(span("word", x, 700.0, 0.0, font_size));
            x = x - font_size - 1.0;
        }
        assert!(
            !is_fragmented_span_list(&spans),
            "word-level spans (> 3 chars) must never trigger fragmentation detection"
        );
    }

    #[test]
    fn large_y_gap_not_classified_as_same_line() {
        let spans = vec![span("A", 300.0, 700.0, 0.0, 12.0), span("B", 50.0, 686.0, 0.0, 12.0)];
        assert!(
            !is_fragmented_span_list(&spans),
            "14 pt y-gap must not be classified as same-line (MAX_GLYPH_JITTER_PT={MAX_GLYPH_JITTER_PT})"
        );
    }

    #[test]
    fn empty_spans_returns_false() {
        assert!(!is_fragmented_span_list(&[]));
    }

    #[test]
    fn single_span_returns_false() {
        assert!(!is_fragmented_span_list(&[span("A", 100.0, 700.0, 0.0, 12.0)]));
    }

    #[test]
    fn detached_subscripts_are_reinserted_into_chemical_formula() {
        let spans = vec![
            span_with_width("H", 100.0, 100.0, 6.0, 10.0, 10.0),
            span_with_width("SO", 108.0, 100.0, 12.0, 10.0, 10.0),
            span_with_width("solution", 124.0, 100.0, 36.0, 10.0, 10.0),
            span_with_width("2", 106.0, 96.0, 2.0, 6.0, 6.0),
            span_with_width("4", 120.0, 96.0, 2.0, 6.0, 6.0),
        ];

        assert_eq!(assemble_page_text(&spans), "H2SO4 solution");
    }

    #[test]
    fn detached_phone_suffix_is_reinserted_without_space() {
        let spans = vec![
            span_with_width("273.879.750", 100.0, 100.0, 60.0, 10.0, 10.0),
            span_with_width("Population", 100.0, 75.0, 45.0, 10.0, 10.0),
            span_with_width("1", 160.0, 103.0, 3.0, 6.0, 6.0),
        ];

        assert_eq!(assemble_page_text(&spans), "273.879.7501\n\nPopulation");
    }

    #[test]
    fn detached_final_glyph_is_reinserted_into_word() {
        let spans = vec![
            span_with_width("eli", 100.0, 100.0, 15.0, 10.0, 10.0),
            span_with_width("Table", 40.0, 75.0, 25.0, 10.0, 10.0),
            span_with_width("t", 115.0, 100.0, 5.0, 10.0, 10.0),
        ];

        assert_eq!(assemble_page_text(&spans), "elit\n\nTable");
    }

    #[test]
    fn far_left_reset_starts_new_row_even_when_vertical_bands_overlap() {
        let spans = vec![
            span_with_width("1.000", 500.0, 100.0, 30.0, 10.0, 10.0),
            span_with_width("002", 30.0, 99.0, 18.0, 10.0, 10.0),
        ];

        assert_eq!(assemble_page_text(&spans), "1.000\n002");
    }

    #[test]
    fn far_left_reset_does_not_split_rtl_text() {
        let mut next = span_with_width("العالم", 430.0, 100.0, 35.0, 10.0, 10.0);
        next.split_boundary_before = true;
        let spans = vec![span_with_width("مرحبا", 500.0, 100.0, 30.0, 10.0, 10.0), next];

        assert_eq!(assemble_page_text(&spans), "مرحبا العالم");
    }

    #[test]
    fn far_left_reset_respects_rtl_span_metadata_for_ascii_text() {
        let mut previous = span_with_width("first", 500.0, 100.0, 30.0, 10.0, 10.0);
        previous.rtl_draw_logical = true;
        let mut next = span_with_width("second", 430.0, 100.0, 35.0, 10.0, 10.0);
        next.rtl_draw_logical = true;
        next.split_boundary_before = true;

        assert_eq!(assemble_page_text(&[previous, next]), "first second");
    }

    #[test]
    fn far_left_reset_does_not_split_ascii_numbers_on_rtl_page() {
        let mut number = span_with_width("123", 500.0, 100.0, 20.0, 10.0, 10.0);
        number.split_boundary_before = true;
        let mut next_number = span_with_width("456", 430.0, 100.0, 20.0, 10.0, 10.0);
        next_number.split_boundary_before = true;
        let spans = vec![
            span_with_width("مرحبا", 570.0, 100.0, 30.0, 10.0, 10.0),
            number,
            next_number,
        ];

        assert_eq!(assemble_page_text(&spans), "مرحبا 123 456");
    }

    #[test]
    fn moderate_math_backtrack_does_not_start_new_row() {
        let mut denominator = span_with_width("denominator", 65.0, 96.0, 55.0, 10.0, 10.0);
        denominator.split_boundary_before = true;
        let spans = vec![
            span_with_width("numerator", 100.0, 104.0, 45.0, 10.0, 10.0),
            denominator,
        ];

        assert_eq!(assemble_page_text(&spans), "numerator denominator");
    }

    #[test]
    fn far_left_reset_does_not_split_rotated_text() {
        let mut previous = span_with_width("first", 500.0, 100.0, 30.0, 10.0, 10.0);
        previous.rotation_degrees = 90.0;
        let mut next = span_with_width("second", 430.0, 100.0, 35.0, 10.0, 10.0);
        next.rotation_degrees = 90.0;
        next.split_boundary_before = true;

        assert_eq!(assemble_page_text(&[previous, next]), "first second");
    }

    #[test]
    fn inline_fragment_anchor_rejects_non_ltr_geometry() {
        let mut anchor = span_with_width("word", 100.0, 100.0, 30.0, 10.0, 10.0);
        anchor.rtl_draw_logical = true;
        let mut fragment = span_with_width("2", 130.0, 100.0, 3.0, 6.0, 6.0);
        fragment.rtl_draw_logical = true;
        let spans = vec![anchor, fragment];

        assert_eq!(find_inline_fragment_anchor(1, &spans, &[None, None]), None);
    }

    #[test]
    fn inline_fragment_anchor_search_is_local() {
        let mut spans = vec![span_with_width("anchor", 100.0, 100.0, 30.0, 10.0, 10.0)];
        spans.extend(
            (0..=MAX_INLINE_FRAGMENT_ANCHOR_LOOKBACK)
                .map(|index| span_with_width("filler", 300.0, index as f32, 30.0, 10.0, 10.0)),
        );
        spans.push(span_with_width("2", 130.0, 100.0, 3.0, 6.0, 6.0));
        let anchors = vec![None; spans.len()];

        assert_eq!(find_inline_fragment_anchor(spans.len() - 1, &spans, &anchors), None);
    }

    #[test]
    fn split_boundary_before_forces_space_between_adjacent_spans() {
        let mut next = span_with_width("002", 130.0, 100.0, 18.0, 10.0, 10.0);
        next.split_boundary_before = true;
        let spans = vec![span_with_width("1.000", 100.0, 100.0, 30.0, 10.0, 10.0), next];

        assert_eq!(assemble_page_text(&spans), "1.000 002");
    }

    #[test]
    fn line_local_repair_preserves_column_aware_order() {
        let spans = vec![
            span_with_width("left-top", 40.0, 100.0, 40.0, 10.0, 10.0),
            span_with_width("left-bottom", 40.0, 80.0, 50.0, 10.0, 10.0),
            span_with_width("right-top", 300.0, 100.0, 45.0, 10.0, 10.0),
            span_with_width("right-bottom", 300.0, 80.0, 55.0, 10.0, 10.0),
        ];

        assert_eq!(
            assemble_page_text(&spans),
            "left-top\n\nleft-bottom\n\nright-top\n\nright-bottom"
        );
    }

    #[test]
    fn sparse_two_column_prose_reorders_by_column() {
        let mut spans = vec![
            span_with_width("The committee reviewed the annual", 60.0, 712.0, 175.0, 11.0, 11.0),
            span_with_width("approved the budget for the", 330.0, 712.0, 145.0, 11.0, 11.0),
            span_with_width("report and", 60.0, 698.0, 52.0, 11.0, 11.0),
            span_with_width("coming fiscal year.", 330.0, 698.0, 92.0, 11.0, 11.0),
        ];

        assert!(reorder_sparse_two_column_page(&mut spans, 612.0));

        assert_eq!(
            spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>(),
            [
                "The committee reviewed the annual",
                "report and",
                "approved the budget for the",
                "coming fiscal year."
            ]
        );
    }

    #[test]
    fn sparse_two_column_table_keeps_row_order() {
        let mut spans = vec![
            span_with_width(
                "Regional revenue for the northern market.",
                60.0,
                712.0,
                210.0,
                11.0,
                11.0,
            ),
            span_with_width("Annual total for the current period.", 330.0, 712.0, 190.0, 11.0, 11.0),
            span_with_width(
                "Operating expense for the northern market.",
                60.0,
                698.0,
                220.0,
                11.0,
                11.0,
            ),
            span_with_width(
                "Quarterly total for the current period.",
                330.0,
                698.0,
                200.0,
                11.0,
                11.0,
            ),
        ];
        let original = spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>();

        assert!(!reorder_sparse_two_column_page(&mut spans, 612.0));

        assert_eq!(
            spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>(),
            original.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }

    #[test]
    fn sparse_verbose_form_keeps_row_order() {
        let mut spans = vec![
            span_with_width(
                "Account holder full legal name appears here:",
                60.0,
                712.0,
                215.0,
                11.0,
                11.0,
            ),
            span_with_width(
                "Mailing address for all official correspondence:",
                330.0,
                712.0,
                225.0,
                11.0,
                11.0,
            ),
            span_with_width(
                "Emergency contact relationship and telephone number:",
                60.0,
                698.0,
                235.0,
                11.0,
                11.0,
            ),
            span_with_width(
                "Preferred delivery method for annual notices:",
                330.0,
                698.0,
                215.0,
                11.0,
                11.0,
            ),
        ];
        let original = spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>();

        assert!(!reorder_sparse_two_column_page(&mut spans, 612.0));
        assert_eq!(
            spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>(),
            original.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }

    #[test]
    fn sparse_lowercase_table_keeps_row_order() {
        let mut spans = vec![
            span_with_width(
                "regional revenue for the northern market",
                60.0,
                712.0,
                210.0,
                11.0,
                11.0,
            ),
            span_with_width("annual total for the current period", 330.0, 712.0, 190.0, 11.0, 11.0),
            span_with_width(
                "operating expense for the northern market",
                60.0,
                698.0,
                220.0,
                11.0,
                11.0,
            ),
            span_with_width(
                "quarterly total for the current period.",
                330.0,
                698.0,
                200.0,
                11.0,
                11.0,
            ),
        ];
        let original = spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>();

        assert!(!reorder_sparse_two_column_page(&mut spans, 612.0));
        assert_eq!(
            spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>(),
            original.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }
}
