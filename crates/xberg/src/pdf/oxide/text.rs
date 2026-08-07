//! PDF text extraction using the pdf_oxide backend.

use super::OxideDocument;
use super::span_geometry::{
    has_same_rotation, is_horizontal_ltr, is_ltr_writing_mode, upright_advance_extent, upright_cross_extent,
};
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
            rotation_degrees: span.rotation_degrees,
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

    // Issue #67: default-off optional-content (OCG/layer) groups per
    // `/OCProperties/D` (ISO 32000-1:2008 §8.11.4). Computed once per
    // document; empty for the common case of no `/OCProperties`.
    let excluded_layers = pdf_oxide::optional_content::compute_default_off_ocgs(&doc.doc);

    let mut content = String::new();
    let mut total_sample_size = 0usize;
    let mut sample_count = 0;

    for page_idx in 0..page_count {
        let page_text = extract_page_text_column_aware(&mut doc.doc, page_idx, &excluded_layers)?;

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

    // Issue #67: see `extract_text_fast_path` for rationale.
    let excluded_layers = pdf_oxide::optional_content::compute_default_off_ocgs(&doc.doc);

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

        let page_text = extract_page_text_column_aware(&mut doc.doc, page_idx, &excluded_layers)?;

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

/// Do the two spans share a line?
///
/// Measured on each span's own cross axis so that a 90-degree rotated pair,
/// whose shared baseline is a page-x column rather than a page-y row, is still
/// recognised as one line. Identical to the previous page-y test for unrotated
/// spans. Only meaningful for spans of equal rotation; callers check that.
fn spans_overlap_on_cross_axis(first: &pdf_oxide::layout::TextSpan, second: &pdf_oxide::layout::TextSpan) -> bool {
    let (first_low, first_high) = upright_cross_extent(first);
    let (second_low, second_high) = upright_cross_extent(second);
    first_high.min(second_high) > first_low.max(second_low)
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

/// Find the parent word a short detached fragment should rejoin.
///
/// Gated on the writing mode only (`wmode` / `rtl_draw_logical`). Rotation is
/// deliberately *not* a reason to refuse the join: a rotated table header is
/// horizontal LTR text painted along a rotated baseline, and refusing to anchor
/// its fragments is what leaves rotated tables glued and word-reversed
/// (GitHub #1358). The candidate must still carry the *same* rotation as the
/// fragment, and all gap arithmetic runs in that rotation's upright frame.
fn find_inline_fragment_anchor(
    index: usize,
    spans: &[pdf_oxide::layout::TextSpan],
    anchors: &[Option<usize>],
) -> Option<usize> {
    let span = &spans[index];
    if span.split_boundary_before
        || !is_short_inline_fragment(span)
        || !is_ltr_writing_mode(span)
        || has_rtl_or_bidi_content(&span.text)
    {
        return None;
    }

    let (span_start, _) = upright_advance_extent(span);
    let search_start = index.saturating_sub(MAX_INLINE_FRAGMENT_ANCHOR_LOOKBACK);
    (search_start..index)
        .filter(|candidate_index| anchors[*candidate_index].is_none())
        .filter_map(|candidate_index| {
            let candidate = &spans[candidate_index];
            if !is_ltr_writing_mode(candidate)
                || has_rtl_or_bidi_content(&candidate.text)
                || !has_same_rotation(candidate, span)
                || !spans_overlap_on_cross_axis(candidate, span)
            {
                return None;
            }
            let (_, candidate_end) = upright_advance_extent(candidate);
            let gap = span_start - candidate_end;
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
            // Along each fragment's own advance axis, so rotated fragments are
            // re-inserted in reading order rather than page-x order.
            let (first_start, _) = upright_advance_extent(&spans[*first]);
            let (second_start, _) = upright_advance_extent(&spans[*second]);
            first_start
                .partial_cmp(&second_start)
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

    // A change of text-matrix rotation is a hard block boundary. pdf_oxide lifts
    // rotated runs out of the horizontal flow and appends them as their own
    // blocks, and the two bboxes are flattened onto different axes, so no gap
    // arithmetic across the boundary is meaningful. This is also what keeps an
    // upright running footer readable on a page whose body is rotated.
    if !has_same_rotation(previous, span) {
        text.push_str("\n\n");
        return;
    }

    // Everything below runs in the pair's shared upright frame: identical to the
    // raw page axes when the pair is unrotated, axis-swapped when it is not.
    let (previous_start, previous_end) = upright_advance_extent(previous);
    let (span_start, _) = upright_advance_extent(span);
    let (previous_baseline, _) = upright_cross_extent(previous);
    let (span_baseline, _) = upright_cross_extent(span);
    let baseline_gap = (previous_baseline - span_baseline).abs();

    let reset_threshold = previous.font_size.max(span.font_size) * ROW_RESET_MIN_BACKTRACK_EMS;
    let is_ltr_pair = is_ltr_writing_mode(previous)
        && is_ltr_writing_mode(span)
        && !has_rtl_or_bidi_content(&previous.text)
        && !has_rtl_or_bidi_content(&span.text);
    if allow_ltr_row_resets && is_ltr_pair && span_start < previous_start - reset_threshold {
        if baseline_gap > paragraph_gap_threshold {
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

    let effective_height = span.bbox.height.max(previous.bbox.height).max(span.font_size * 0.5);
    if baseline_gap < effective_height * 0.5 {
        if span_start - previous_end > span.font_size * 0.15 {
            text.push(' ');
        }
    } else if baseline_gap > paragraph_gap_threshold {
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
    let has_terminal = |span: &&pdf_oxide::layout::TextSpan| span.text.trim_end().ends_with(['.', '!', '?']);
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

// Issue #1397: a dense two-column body (a full page of prose, not the guarded
// four-span sentence above) is never split by pdf_oxide's own `ColumnAware`
// XY-Cut on some documents, so xberg's span-level assembler falls through to
// full-page-width Y order — welding left- and right-column lines at the same
// height into one interleaved element, mid-sentence, and welding distinct
// per-column headings (e.g. "Funding" + "References") into one heading
// element. No downstream reordering pass can repair (2): the interleaving is
// already baked into the element text by the time it is produced.
const MIN_DENSE_COLUMN_CONTENT_WIDTH_PTS: f32 = 200.0;
// 2%, not 3%. On the reporting document (A4, 595pt, columns at x=37.6 and
// x=306.6) symmetric margins put the left column's right edge at 288.4, so the
// real gutter is ~18.2pt — against a 3% threshold of 17.85pt that is a 0.35pt
// margin, and any page whose widest left-column line falls a point short of
// full justification would silently stop being repaired. 2% gives 11.9pt on
// A4, still far above the intra-line word spacing (~3-5pt at a 10pt font) that
// is the only thing this must not mistake for a column boundary.
const MIN_DENSE_COLUMN_GUTTER_FRACTION: f32 = 0.02;
const MIN_DENSE_COLUMN_GUTTER_PTS: f32 = 10.0;
const MIN_DENSE_COLUMN_SPANS_PER_SIDE: usize = 6;
// A full-width furniture span (running header/footer, page-wide rule,
// full-width title) spans nearly the entire printable width regardless of
// the two-column layout beneath it, whereas a genuine column is bounded by
// the page margins AND the gutter and can never reach much past ~45% of the
// page width even on a page with unusually narrow margins. On the reporting
// document from the worked example above (A4, 595pt wide, columns at
// x=37.6/x=306.6), each column is 250.8pt wide = 42.2% of page width, while
// a running header spanning x=37.6..557 is 519.4pt = 87.3% of page width.
// 0.55 sits 13 points above the column ceiling (headroom for
// justification/kerning noise on an unusually wide column line) and over 30
// points below a typical full-width furniture span, so it cleanly separates
// the two without needing per-document calibration.
const FULL_WIDTH_FURNITURE_FRACTION: f32 = 0.55;

/// Find the single widest vertical gutter that splits `spans` into two
/// non-empty, non-overlapping-in-x halves, or `None` if no gutter wide enough
/// to plausibly be a column boundary exists.
///
/// This is a horizontal-projection sweep: sort spans by left edge, track the
/// running rightmost edge seen so far, and record the largest gap between that
/// running edge and the next span's left edge. A real two-column body has
/// exactly one such gap spanning the page (the gutter); ordinary word/line
/// spacing inside a column never produces a gap this wide. Because the sweep
/// tracks the *running maximum* right edge, every span preceding the gap is
/// guaranteed to lie entirely left of it and every span following the gap
/// lies entirely right of it, so the returned midpoint is a clean partition
/// boundary.
///
/// UPDATE (GH#1397 follow-up): a single span that crosses the gutter — a
/// full-width title, a centred running footer, a page-wide rule — used to
/// close the projection gap and suppress the split for that whole page. Since
/// real two-column documents almost always carry exactly this kind of
/// full-width furniture on every page, that made the repair silently a
/// no-op on the documents it exists for. This is now handled: any span at
/// least `FULL_WIDTH_FURNITURE_FRACTION` of the page width is excluded from
/// the gutter projection below (and, in `reorder_dense_two_column_page`, from
/// the column partition), so a header/footer/rule/title no longer masks a
/// real gutter between two columns of ordinary body content.
///
/// KNOWN LIMITATION (still unhandled): this is a single global width
/// threshold, not vertical segmentation. A full-width span that is
/// nonetheless *narrower* than `FULL_WIDTH_FURNITURE_FRACTION` of the page
/// (e.g. a furniture line that doesn't reach quite as far as the body
/// columns' combined span) still closes the gap and suppresses the whole-page
/// repair, exactly as before. Likewise, furniture interleaved vertically
/// between column content (e.g. a rule between every few paragraphs) is only
/// classified relative to the *combined* vertical extent of the two columns
/// (see `dense_two_column_sort_key`), so a piece of furniture that sits
/// strictly between the columns' top and bottom lines is emitted between the
/// two columns -- after all of the left, before all of the right -- rather
/// than at its true interleaved position. True per-band vertical segmentation
/// remains a follow-up.
fn dense_column_split_x(spans: &[pdf_oxide::layout::TextSpan], page_width: f32) -> Option<f32> {
    if spans.len() < 2 {
        return None;
    }
    let content_left = spans.iter().map(|span| span.bbox.x).fold(f32::INFINITY, f32::min);
    let content_right = spans
        .iter()
        .map(|span| span.bbox.x + span.bbox.width)
        .fold(f32::NEG_INFINITY, f32::max);
    if content_right - content_left < MIN_DENSE_COLUMN_CONTENT_WIDTH_PTS {
        return None;
    }
    let min_gutter = (page_width * MIN_DENSE_COLUMN_GUTTER_FRACTION).max(MIN_DENSE_COLUMN_GUTTER_PTS);
    let furniture_width = page_width * FULL_WIDTH_FURNITURE_FRACTION;

    let mut edges: Vec<(f32, f32)> = spans
        .iter()
        .filter(|span| span.bbox.width < furniture_width)
        .map(|span| (span.bbox.x, span.bbox.x + span.bbox.width))
        .collect();
    if edges.len() < 2 {
        return None;
    }
    edges.sort_by(|left, right| left.0.total_cmp(&right.0));

    let mut running_right = edges[0].1;
    let mut best_gap = 0.0_f32;
    let mut best_split = None;
    for &(left, right) in &edges[1..] {
        let gap = left - running_right;
        if gap > best_gap {
            best_gap = gap;
            best_split = Some((running_right + left) / 2.0);
        }
        running_right = running_right.max(right);
    }

    if best_gap < min_gutter { None } else { best_split }
}

/// Reorder a dense two-column page (issue #1397) that pdf_oxide's own
/// `ColumnAware` reading order fails to split.
///
/// Unlike `reorder_sparse_two_column_page` above (which repairs a single
/// guarded four-span sentence), this targets the common case of a full page
/// of two-column body text. It finds the widest vertical gutter splitting the
/// page into two halves, classifies each half with pdf_oxide's own
/// `classify_region`, and only reorders column-major when BOTH halves
/// positively identify as reorderable prose/reference
/// (`RegionClass::is_reorderable_column`) — tables, forms, and anything
/// ambiguous (`RegionClass::Mixed`) are left untouched, matching
/// `classify_region`'s own contract of degrading gracefully to the
/// pre-existing geometric behaviour on any doubt.
///
/// Full-width furniture spans (`FULL_WIDTH_FURNITURE_FRACTION` of the page
/// width or wider) are excluded from both the column partition and the
/// `classify_region` calls — they are not folded into either column — and are
/// instead placed relative to the combined vertical extent of the two columns
/// by `dense_two_column_sort_key`: furniture above both columns' top line
/// (a running header, or a full-width heading sitting above the columns even
/// mid-page) sorts before all column content, and furniture below both
/// columns' bottom line (a running footer) sorts after it.
pub(crate) fn reorder_dense_two_column_page(spans: &mut [pdf_oxide::layout::TextSpan], page_width: f32) -> bool {
    let Some(split_x) = dense_column_split_x(spans, page_width) else {
        return false;
    };
    let furniture_width = page_width * FULL_WIDTH_FURNITURE_FRACTION;

    let mut left_indices = Vec::new();
    let mut right_indices = Vec::new();
    for (index, span) in spans.iter().enumerate() {
        if span.bbox.width >= furniture_width {
            continue;
        }
        if span.bbox.x < split_x {
            left_indices.push(index);
        } else {
            right_indices.push(index);
        }
    }
    if left_indices.len() < MIN_DENSE_COLUMN_SPANS_PER_SIDE || right_indices.len() < MIN_DENSE_COLUMN_SPANS_PER_SIDE {
        return false;
    }

    let left_class = pdf_oxide::layout::classify_region(spans, &left_indices);
    let right_class = pdf_oxide::layout::classify_region(spans, &right_indices);
    if !left_class.is_reorderable_column() || !right_class.is_reorderable_column() {
        return false;
    }

    let column_top = left_indices
        .iter()
        .chain(right_indices.iter())
        .map(|&index| spans[index].bbox.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let column_bottom = left_indices
        .iter()
        .chain(right_indices.iter())
        .map(|&index| spans[index].bbox.y)
        .fold(f32::INFINITY, f32::min);

    spans.sort_by(|left, right| {
        let left_key = dense_two_column_sort_key(left, split_x, furniture_width, column_top, column_bottom);
        let right_key = dense_two_column_sort_key(right, split_x, furniture_width, column_top, column_bottom);
        left_key
            .0
            .cmp(&right_key.0)
            .then_with(|| left_key.1.total_cmp(&right_key.1))
            .then_with(|| left_key.2.total_cmp(&right_key.2))
            .then_with(|| left_key.3.total_cmp(&right_key.3))
    });
    true
}

/// Sort key for `reorder_dense_two_column_page`'s final ordering.
///
/// Column spans (width below `furniture_width`) get group `1`, column `0.0`
/// (left) or `1.0` (right), then top-to-bottom (descending y, via `-y`), then
/// left-to-right within a line.
///
/// Furniture spans (width at or above `furniture_width`) are never assigned
/// to a column. They get group `0` if they sit above the combined top line of
/// both columns (`y > column_top`: a header, or a full-width heading above
/// the columns), group `2` if they sit below the combined bottom line
/// (`y < column_bottom`: a footer), and group `1` with a neutral `0.5` column
/// otherwise.
///
/// That `0.5` is deterministic, not arbitrary: it sorts between the left
/// column's `0.0` and the right column's `1.0`, so furniture interleaved
/// vertically between the columns' own lines is emitted after the whole left
/// column and before the whole right column, ordered by y among itself. That
/// is not its true reading position — a column-major order has no well-defined
/// slot for it — but it keeps the span out of both columns instead of
/// corrupting one (see the KNOWN LIMITATION note on `dense_column_split_x`).
fn dense_two_column_sort_key(
    span: &pdf_oxide::layout::TextSpan,
    split_x: f32,
    furniture_width: f32,
    column_top: f32,
    column_bottom: f32,
) -> (u8, f32, f32, f32) {
    if span.bbox.width >= furniture_width {
        let group = if span.bbox.y > column_top {
            0
        } else if span.bbox.y < column_bottom {
            2
        } else {
            1
        };
        (group, 0.5, -span.bbox.y, span.bbox.x)
    } else {
        let column = if span.bbox.x >= split_x { 1.0 } else { 0.0 };
        (1, column, -span.bbox.y, span.bbox.x)
    }
}

/// Build a page's `PageText` (spans + derived chars + dimensions), honouring
/// optional-content (OCG/layer) visibility (issue #67).
///
/// `PdfDocument::extract_page_text_with_options` always treats every layer as
/// visible; a default-OFF `/OCProperties` layer that mirrors the page's content
/// (a common PDF-authoring pattern for redlines/translations/print-vs-screen
/// variants) then contributes a second, hidden-in-every-viewer copy of the page
/// text. When `excluded_layers` is non-empty, this instead calls pdf_oxide's
/// filtered span extraction so the surfaced text matches what any viewer
/// actually renders. An empty set is byte-identical to the unfiltered call.
fn page_text_with_options_excluding_layers(
    doc: &pdf_oxide::PdfDocument,
    page_index: usize,
    excluded_layers: &std::collections::HashSet<String>,
) -> pdf_oxide::error::Result<pdf_oxide::layout::PageText> {
    if excluded_layers.is_empty() {
        return doc.extract_page_text_with_options(page_index, ReadingOrder::ColumnAware);
    }

    let spans = doc.extract_spans_filtered_with_reading_order(
        page_index,
        ReadingOrder::ColumnAware,
        excluded_layers.clone(),
        Default::default(),
    )?;
    let chars: Vec<pdf_oxide::layout::TextChar> = spans.iter().flat_map(|s| s.to_chars()).collect();
    let (_, _, page_width, page_height) = doc.get_page_media_box(page_index)?;

    Ok(pdf_oxide::layout::PageText {
        spans,
        chars,
        page_width,
        page_height,
    })
}

/// Extract text from one page with column-aware ordering and guarded repairs.
///
/// Applies sparse-column and glyph-fragmentation repairs before assembling the
/// page text.
fn extract_page_text_column_aware(
    doc: &mut pdf_oxide::PdfDocument,
    page_index: usize,
    excluded_layers: &std::collections::HashSet<String>,
) -> Result<String> {
    let widgets = collect_widget_field_values(doc, page_index);

    let mut page_text_data = super::guard_oxide_panic(
        || {
            page_text_with_options_excluding_layers(doc, page_index, excluded_layers).map_err(|e| {
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
    reorder_dense_two_column_page(&mut page_text_data.spans, page_text_data.page_width);

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

    /// A span painted with a rotated text matrix. `x`/`y` stay page-space (that
    /// is what pdf_oxide reports); `width` is the glyph-advance run along the
    /// rotated baseline and `height` the font extent across it.
    fn rotated_span(text: &str, x: f32, y: f32, width: f32, height: f32, rotation_degrees: f32) -> TextSpan {
        let mut span = span_with_width(text, x, y, width, height, height);
        span.rotation_degrees = rotation_degrees;
        span
    }

    /// #1358 / #294 — a detached fragment of a rotated word must rejoin its
    /// parent instead of being stranded at the end of the run.
    ///
    /// Revert check (expect RED): restore the `rotation_degrees.abs() <=
    /// f32::EPSILON` term in `span_geometry::is_ltr_writing_mode`'s callers —
    /// i.e. use `is_horizontal_ltr` again in `find_inline_fragment_anchor` — and
    /// this asserts `"MotorcrafPremiumt"`.
    #[test]
    fn should_rejoin_detached_fragment_of_rotated_word_when_rotation_matches() {
        let spans = vec![
            rotated_span("Motorcraf", 400.0, 100.0, 45.0, 10.0, 90.0),
            rotated_span("Premium", 400.0, 155.0, 40.0, 10.0, 90.0),
            rotated_span("t", 400.0, 145.0, 5.0, 10.0, 90.0),
        ];

        assert_eq!(assemble_page_text(&spans), "Motorcraft Premium");
    }

    /// #1358 / #294 — the anchor must still refuse to bridge two different
    /// rotations, so a rotated fragment never steals an upright parent.
    #[test]
    fn should_not_anchor_fragment_across_differing_rotations() {
        let spans = vec![
            span_with_width("Motorcraf", 400.0, 100.0, 45.0, 10.0, 10.0),
            rotated_span("t", 445.0, 100.0, 5.0, 10.0, 90.0),
        ];

        assert_eq!(find_inline_fragment_anchor(1, &spans, &[None, None]), None);
    }

    /// #1358 / #293 — a sideways table reads down its own rows, not across
    /// them: words on one rotated line are space-joined and the next rotated
    /// line starts a new line.
    ///
    /// Revert check (expect RED): restore the page-axis `y_gap` / `bbox.x`
    /// arithmetic in `append_span_separator` and this asserts
    /// `"Enginecoolant\n\n18.6\n\nquarts"` — every word of a line glued, every
    /// line boundary turned into a paragraph break.
    #[test]
    fn should_read_rotated_table_rows_along_their_own_axis() {
        let spans = vec![
            rotated_span("Engine", 400.0, 100.0, 30.0, 10.0, 90.0),
            rotated_span("coolant", 400.0, 132.0, 32.0, 10.0, 90.0),
            rotated_span("18.6", 388.0, 100.0, 22.0, 10.0, 90.0),
            rotated_span("quarts", 388.0, 124.0, 30.0, 10.0, 90.0),
        ];

        assert_eq!(assemble_page_text(&spans), "Engine coolant\n18.6 quarts");
    }

    /// #1358 / #293 — the mixed page. A whole-page rotation transform would fix
    /// the rotated body and break the upright running footer; only a per-run
    /// frame reads both correctly, with a hard block break between them.
    ///
    /// Revert check (expect RED): with the page-axis arithmetic restored this
    /// asserts `"Enginecoolant\n\n18.6\n\nquarts\n\nPage 264"`.
    #[test]
    fn should_read_rotated_body_and_upright_footer_on_same_page() {
        let spans = vec![
            rotated_span("Engine", 400.0, 100.0, 30.0, 10.0, 90.0),
            rotated_span("coolant", 400.0, 132.0, 32.0, 10.0, 90.0),
            rotated_span("18.6", 388.0, 100.0, 22.0, 10.0, 90.0),
            rotated_span("quarts", 388.0, 124.0, 30.0, 10.0, 90.0),
            span_with_width("Page", 60.0, 40.0, 25.0, 10.0, 10.0),
            span_with_width("264", 88.0, 40.0, 15.0, 10.0, 10.0),
        ];

        assert_eq!(assemble_page_text(&spans), "Engine coolant\n18.6 quarts\n\nPage 264");
    }

    /// #1358 — upright pages must be byte-identical after the change. Two
    /// wrapped body lines plus a paragraph break, all rotation 0.
    #[test]
    fn should_not_change_upright_page_assembly() {
        let spans = vec![
            span_with_width("Engine", 60.0, 700.0, 30.0, 10.0, 10.0),
            span_with_width("coolant", 92.0, 700.0, 32.0, 10.0, 10.0),
            span_with_width("18.6", 60.0, 688.0, 22.0, 10.0, 10.0),
            span_with_width("quarts", 84.0, 688.0, 30.0, 10.0, 10.0),
            span_with_width("Next", 60.0, 640.0, 25.0, 10.0, 10.0),
        ];

        assert_eq!(assemble_page_text(&spans), "Engine coolant\n18.6 quarts\n\nNext");
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

    /// Build the interleaved (pre-fix) span order a dense two-column page
    /// naturally arrives in: sorted by full-page-width Y, so left- and
    /// right-column lines at the same height are adjacent. Each column is one
    /// coherent paragraph behind a one-word heading, mirroring GH#1397
    /// ("Funding" / "References" welded together at the same height).
    fn dense_two_column_spans() -> Vec<TextSpan> {
        const LEFT_X: f32 = 60.0;
        const RIGHT_X: f32 = 320.0;
        let left_heading = span_with_width("Funding", LEFT_X, 830.0, 70.0, 11.0, 11.0);
        let right_heading = span_with_width("References", RIGHT_X, 830.0, 90.0, 11.0, 11.0);
        let left_body = [
            "The committee reviewed annual budget totals",
            "and approved new funding for the coming year",
            "after several rounds of careful review by",
            "senior staff members from every department",
            "who evaluated priorities across the whole",
            "organization before reaching a final decision",
            "that reflected both short and long term goals",
            "for sustainable growth across all programs",
        ];
        let right_body = [
            "Numerous studies have examined similar",
            "programs across comparable institutions",
            "using consistent methodology and controls",
            "for measuring outcomes over multiple years",
            "researchers found consistent positive trends",
            "supporting continued investment going forward",
            "additional citations appear in the appendix",
            "for readers seeking further detail here",
        ];

        let mut spans = vec![left_heading, right_heading];
        for (row, (left_line, right_line)) in left_body.iter().copied().zip(right_body.iter().copied()).enumerate() {
            let y = 816.0 - row as f32 * 14.0;
            spans.push(span_with_width(left_line, LEFT_X, y, 200.0, 11.0, 11.0));
            spans.push(span_with_width(right_line, RIGHT_X, y, 190.0, 11.0, 11.0));
        }
        spans
    }

    #[test]
    fn dense_two_column_prose_reorders_by_column() {
        let mut spans = dense_two_column_spans();

        assert!(reorder_dense_two_column_page(&mut spans, 612.0));

        let texts = spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                "Funding",
                "The committee reviewed annual budget totals",
                "and approved new funding for the coming year",
                "after several rounds of careful review by",
                "senior staff members from every department",
                "who evaluated priorities across the whole",
                "organization before reaching a final decision",
                "that reflected both short and long term goals",
                "for sustainable growth across all programs",
                "References",
                "Numerous studies have examined similar",
                "programs across comparable institutions",
                "using consistent methodology and controls",
                "for measuring outcomes over multiple years",
                "researchers found consistent positive trends",
                "supporting continued investment going forward",
                "additional citations appear in the appendix",
                "for readers seeking further detail here",
            ]
        );
    }

    #[test]
    fn dense_two_column_prose_assembles_without_interleaving_or_heading_weld() {
        let mut spans = dense_two_column_spans();

        assert!(reorder_dense_two_column_page(&mut spans, 612.0));

        assert_eq!(
            assemble_page_text(&spans),
            "Funding\n\
             The committee reviewed annual budget totals\n\
             and approved new funding for the coming year\n\
             after several rounds of careful review by\n\
             senior staff members from every department\n\
             who evaluated priorities across the whole\n\
             organization before reaching a final decision\n\
             that reflected both short and long term goals\n\
             for sustainable growth across all programs\n\n\
             References\n\
             Numerous studies have examined similar\n\
             programs across comparable institutions\n\
             using consistent methodology and controls\n\
             for measuring outcomes over multiple years\n\
             researchers found consistent positive trends\n\
             supporting continued investment going forward\n\
             additional citations appear in the appendix\n\
             for readers seeking further detail here"
        );
    }

    #[test]
    fn dense_two_column_table_keeps_row_order() {
        const LEFT_X: f32 = 60.0;
        const RIGHT_X: f32 = 320.0;
        let left_body = [
            "The committee reviewed annual budget totals",
            "and approved new funding for the coming year",
            "after several rounds of careful review by",
            "senior staff members from every department",
            "who evaluated priorities across the whole",
            "organization before reaching a final decision",
            "that reflected both short and long term goals",
            "for sustainable growth across all programs",
        ];
        let right_cells = ["12.3", "45.6", "78.9", "10.1", "21.2", "33.4", "45.5", "67.8"];

        let mut spans = Vec::new();
        for (row, (left_line, right_cell)) in left_body.iter().copied().zip(right_cells.iter().copied()).enumerate() {
            let y = 816.0 - row as f32 * 14.0;
            spans.push(span_with_width(left_line, LEFT_X, y, 200.0, 11.0, 11.0));
            spans.push(span_with_width(right_cell, RIGHT_X, y, 30.0, 11.0, 11.0));
        }
        let original = spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>();

        assert!(!reorder_dense_two_column_page(&mut spans, 612.0));
        assert_eq!(
            spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>(),
            original.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }

    /// GH#1397 follow-up: a running header and footer each cross the gutter
    /// (width 497pt on a 612pt page = 81.2%, well past
    /// `FULL_WIDTH_FURNITURE_FRACTION`'s 55% threshold), which used to close
    /// the projection gap and suppress the whole-page repair. The header and
    /// footer must now be excluded from the gutter search and the column
    /// partition, and the repair must still fire: header first, then the
    /// entire left column, then the entire right column, then the footer.
    #[test]
    fn dense_two_column_prose_reorders_around_header_and_footer() {
        let mut spans = vec![span_with_width(
            "Quarterly Report - Internal Distribution Only",
            60.0,
            850.0,
            497.0,
            11.0,
            11.0,
        )];
        spans.extend(dense_two_column_spans());
        spans.push(span_with_width("Page 1 of 12", 60.0, 700.0, 497.0, 11.0, 11.0));

        assert!(reorder_dense_two_column_page(&mut spans, 612.0));

        let texts = spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                "Quarterly Report - Internal Distribution Only",
                "Funding",
                "The committee reviewed annual budget totals",
                "and approved new funding for the coming year",
                "after several rounds of careful review by",
                "senior staff members from every department",
                "who evaluated priorities across the whole",
                "organization before reaching a final decision",
                "that reflected both short and long term goals",
                "for sustainable growth across all programs",
                "References",
                "Numerous studies have examined similar",
                "programs across comparable institutions",
                "using consistent methodology and controls",
                "for measuring outcomes over multiple years",
                "researchers found consistent positive trends",
                "supporting continued investment going forward",
                "additional citations appear in the appendix",
                "for readers seeking further detail here",
                "Page 1 of 12",
            ]
        );
    }

    /// GH#1397 follow-up: a full-width heading can sit well above the two
    /// columns without being at the very top edge of the page ("mid-page"
    /// furniture) — e.g. a document title printed a few lines above where
    /// the two-column body starts. It must stay above BOTH columns in the
    /// output, exactly like a page-top running header, since the rule is
    /// purely relative to the columns' own vertical extent, not to any
    /// absolute page position.
    #[test]
    fn dense_two_column_prose_keeps_midpage_heading_above_both_columns() {
        let mut spans = vec![span_with_width(
            "Annual Committee Findings",
            60.0,
            840.0,
            497.0,
            11.0,
            11.0,
        )];
        spans.extend(dense_two_column_spans());

        assert!(reorder_dense_two_column_page(&mut spans, 612.0));

        let texts = spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>();
        assert_eq!(texts[0], "Annual Committee Findings");
        let heading_index = 0;
        let funding_index = texts.iter().position(|&text| text == "Funding").unwrap();
        let references_index = texts.iter().position(|&text| text == "References").unwrap();
        assert!(heading_index < funding_index && heading_index < references_index);
    }

    /// Regression guard: a genuine single-column page with both wide
    /// (near-furniture-width) and narrow lines must NOT be split. All lines
    /// share the same left edge (there is only one column to begin with), so
    /// excluding the wide lines as "furniture" from the gutter search must
    /// not manufacture an artificial gap among the remaining narrow lines.
    /// Splitting a genuinely single-column page scrambles correct output,
    /// which is worse than leaving the (non-existent) repair unapplied.
    #[test]
    fn single_column_page_with_wide_and_narrow_lines_is_not_split() {
        const COLUMN_X: f32 = 60.0;
        let lines: [(&str, f32); 8] = [
            ("This is a long justified line of body text filling", 470.0),
            ("the page width almost completely from margin", 470.0),
            ("to margin, as ordinary single-column prose does", 470.0),
            ("Short line.", 90.0),
            ("Another full-width line of ordinary body text here", 470.0),
            ("Brief.", 90.0),
            ("A further wide line completing this single paragraph", 470.0),
            ("End.", 90.0),
        ];
        let mut spans = Vec::new();
        for (row, (text, width)) in lines.iter().enumerate() {
            let y = 800.0 - row as f32 * 14.0;
            spans.push(span_with_width(text, COLUMN_X, y, *width, 11.0, 11.0));
        }
        let original = spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>();

        assert!(!reorder_dense_two_column_page(&mut spans, 612.0));
        assert_eq!(
            spans.iter().map(|span| span.text.as_str()).collect::<Vec<_>>(),
            original.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }
}
