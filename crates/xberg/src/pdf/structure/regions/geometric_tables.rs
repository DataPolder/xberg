//! Geometric table-region fallback (xberg-io/xberg#1316).
//!
//! Under the corrected square-resize RT-DETR preprocessing (commit `8299d3ea`,
//! faithful to the Docling Heron ONNX export), the layout detector no longer
//! emits a `Table` region for thin, sparse, borderless tables — an A4 page
//! squashed to 640×640 vertically compresses a 2–3 row grid below the model's
//! detection sensitivity. Reverting the preprocessing is not an option (it would
//! re-break the official-contract fix), so this module recovers those tables
//! from column-aligned text geometry instead.
//!
//! It synthesizes a `Table` region **only** for pages where the ML detector
//! produced no `Table` region at all — it never overrides the detector where it
//! already fired. The synthesized region is reconstructed through the exact same
//! guarded path as an ML `Table` hint
//! ([`super::tables::extract_tables_from_layout_hints`]), so every #36
//! false-positive guard (`post_process_table`, `is_well_formed_table`, the
//! numeric-exemption prose gate, the code-listing and single-cell-row guards)
//! still decides acceptance. This widens *candidate proposal* only; it never
//! bypasses validation.

use crate::pdf::structure::types::{LayoutHint, LayoutHintClass};
use crate::pdf::table_reconstruct::HocrWord;

/// Minimum vertically-contiguous tabular rows to propose a table.
const MIN_TABLE_ROWS: usize = 3;
/// Minimum consistent column anchors a run must share to propose a table.
/// Three columns is the key precision guard: one- and two-column prose reflows
/// (references, two-column body text) cannot clear it, so they never reach the
/// downstream guards as table candidates.
const MIN_TABLE_COLS: usize = 3;
/// Two column left-edges within this many points are treated as the same column.
const ANCHOR_TOLERANCE_PTS: u32 = 10;
/// A column anchor counts as "consistent" when at least this fraction of the
/// run's rows place a segment at it.
const MIN_ANCHOR_ROW_SUPPORT: f32 = 0.6;
/// A run breaks when the top-to-top pitch to the next tabular row exceeds this
/// multiple of the median word height — keeps a genuine grid together while
/// refusing to bridge a table into distant prose.
const MAX_ROW_PITCH_FACTOR: f32 = 3.5;
/// Rows whose tops differ by at most this multiple of the median word height are
/// the same visual row.
const ROW_GROUPING_FACTOR: f32 = 0.6;
/// Minimum fraction of a run's words that must be numeric-value tokens for the
/// run to be proposed. The #1316 target class — borderless invoice / line-item /
/// metric tables — is numeric, whereas the dominant geometric false positive is
/// alphabetic multi-column prose (two-column academic body text), which cannot
/// clear this bar. The ML detector remains responsible for textual tables; this
/// fallback deliberately covers only the numeric borderless case it regressed.
const MIN_NUMERIC_WORD_FRACTION: f32 = 0.35;
/// Confidence stamped on a synthesized hint. `extract_tables_from_layout_hints`
/// filters hints by `confidence >= min_confidence` (0.5 at every call site), so
/// a synthesized region must clear that bar; the geometric evidence has already
/// been vetted by the row/column thresholds above.
const SYNTHETIC_HINT_CONFIDENCE: f32 = 1.0;

/// A visual row: word indices sharing a top band, plus the row's top anchor.
struct Row {
    word_indices: Vec<usize>,
    top: u32,
}

/// Detect column-aligned multi-row text bands and return them as synthetic
/// `Table` hints in PDF (bottom-origin) coordinates, matching the coordinate
/// contract of ML-produced [`LayoutHint`]s (`hint_img_top = page_height -
/// hint.top`). Returns an empty vector when no band clears the thresholds or the
/// `XBERG_LAYOUT_NO_GEOMETRIC_TABLES` toggle is set.
pub(in crate::pdf::structure) fn detect_geometric_table_hints(words: &[HocrWord], page_height: f32) -> Vec<LayoutHint> {
    if crate::pdf::structure::layout_debug::layout_debug_flags().no_geometric_tables {
        return Vec::new();
    }

    let indexed: Vec<usize> = words
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.text.trim().is_empty())
        .map(|(i, _)| i)
        .collect();
    if indexed.len() < MIN_TABLE_ROWS * MIN_TABLE_COLS {
        return Vec::new();
    }

    let median_height = median_word_height(words, &indexed);
    if median_height == 0 {
        return Vec::new();
    }

    let rows = group_rows(words, &indexed, median_height);
    if rows.len() < MIN_TABLE_ROWS {
        return Vec::new();
    }

    let max_row_pitch = (median_height as f32 * MAX_ROW_PITCH_FACTOR).round() as u32;

    // Split rows into vertically-contiguous runs (top-to-top pitch within
    // `max_row_pitch`), then propose each run whose columns are consistently
    // aligned. Column consistency — not per-row gap segmentation — is the real
    // signal: prose reflows share a left margin but no ≥3 internal columns.
    let mut hints = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    for row_idx in 0..rows.len() {
        let contiguous = run
            .last()
            .map(|&prev| rows[row_idx].top.saturating_sub(rows[prev].top) <= max_row_pitch)
            .unwrap_or(true);
        if !contiguous {
            if let Some(hint) = finalize_run(words, &rows, &run, page_height) {
                hints.push(hint);
            }
            run.clear();
        }
        run.push(row_idx);
    }
    if let Some(hint) = finalize_run(words, &rows, &run, page_height) {
        hints.push(hint);
    }

    hints
}

/// Evaluate a run of vertically-contiguous rows: emit a hint only when the run
/// has enough rows and enough word-left anchors are consistently aligned across
/// them (≥[`MIN_TABLE_COLS`] columns present in ≥[`MIN_ANCHOR_ROW_SUPPORT`] of
/// the rows).
fn finalize_run(words: &[HocrWord], rows: &[Row], run: &[usize], page_height: f32) -> Option<LayoutHint> {
    if run.len() < MIN_TABLE_ROWS {
        return None;
    }

    // Each word's left edge is a candidate column anchor. A cell of multiple
    // words contributes one anchor per word, but only anchors aligned across
    // enough rows survive the support filter, so stray second-word offsets that
    // appear in a single row (e.g. a two-word header cell) drop out.
    let mut anchors: Vec<u32> = Vec::new();
    let mut per_row_lefts: Vec<Vec<u32>> = Vec::with_capacity(run.len());
    for &row_idx in run {
        let mut lefts: Vec<u32> = rows[row_idx].word_indices.iter().map(|&i| words[i].left).collect();
        lefts.sort_unstable();
        anchors.extend(lefts.iter().copied());
        per_row_lefts.push(lefts);
    }
    anchors.sort_unstable();

    let min_support = ((run.len() as f32 * MIN_ANCHOR_ROW_SUPPORT).ceil() as usize).max(2);
    let consistent_columns = count_consistent_anchors(&anchors, &per_row_lefts, min_support);
    if consistent_columns < MIN_TABLE_COLS {
        return None;
    }

    if !run_is_numeric_dominant(words, rows, run) {
        return None;
    }

    Some(run_bounding_hint(words, rows, run, page_height))
}

/// Whether a run's words are numeric-dominant — the separator between the
/// numeric borderless tables this fallback targets (#1316) and the alphabetic
/// multi-column prose that is its dominant false positive.
fn run_is_numeric_dominant(words: &[HocrWord], rows: &[Row], run: &[usize]) -> bool {
    let mut total = 0usize;
    let mut numeric = 0usize;
    for &row_idx in run {
        for &i in &rows[row_idx].word_indices {
            let text = words[i].text.trim();
            if text.is_empty() {
                continue;
            }
            total += 1;
            if is_numeric_token(text) {
                numeric += 1;
            }
        }
    }
    total > 0 && numeric as f32 >= total as f32 * MIN_NUMERIC_WORD_FRACTION
}

/// A token reads as a numeric data value when it carries at least one digit and
/// no alphabetic character — a real number, optionally decorated with grouping,
/// decimal, sign, currency, or percent punctuation. Catches `10.00`, `19%`,
/// `1,234`, `$305,568`, `2024-01`, `(42,253)`; rejects prose words, unit labels
/// like `000s`, and math variables/subscripts like `a1`, `2j`, `γδ` — the latter
/// being the dominant false positive on equation-dense academic pages (#1316).
fn is_numeric_token(text: &str) -> bool {
    let mut has_digit = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if c.is_alphabetic() {
            return false;
        }
    }
    has_digit
}

/// Cluster anchors within [`ANCHOR_TOLERANCE_PTS`] and count clusters that are
/// populated in at least `min_support` distinct rows.
fn count_consistent_anchors(sorted_anchors: &[u32], per_row_starts: &[Vec<u32>], min_support: usize) -> usize {
    let mut cluster_centers: Vec<u32> = Vec::new();
    let mut cluster_start = 0usize;
    while cluster_start < sorted_anchors.len() {
        let base = sorted_anchors[cluster_start];
        let mut cluster_end = cluster_start + 1;
        while cluster_end < sorted_anchors.len()
            && sorted_anchors[cluster_end].saturating_sub(base) <= ANCHOR_TOLERANCE_PTS
        {
            cluster_end += 1;
        }
        let center = sorted_anchors[cluster_start..cluster_end]
            .iter()
            .map(|&v| v as u64)
            .sum::<u64>()
            / (cluster_end - cluster_start) as u64;
        cluster_centers.push(center as u32);
        cluster_start = cluster_end;
    }

    cluster_centers
        .iter()
        .filter(|&&center| {
            let supporting_rows = per_row_starts
                .iter()
                .filter(|starts| starts.iter().any(|&s| s.abs_diff(center) <= ANCHOR_TOLERANCE_PTS))
                .count();
            supporting_rows >= min_support
        })
        .count()
}

/// Bounding hint over every word in the run's rows, in PDF (bottom-origin)
/// coordinates so it matches the ML-hint contract consumed by
/// `extract_tables_from_layout_hints`.
fn run_bounding_hint(words: &[HocrWord], rows: &[Row], run: &[usize], page_height: f32) -> LayoutHint {
    let mut min_left = u32::MAX;
    let mut max_right = 0u32;
    let mut min_top = u32::MAX;
    let mut max_bottom = 0u32;
    for &row_idx in run {
        for &i in &rows[row_idx].word_indices {
            let w = &words[i];
            min_left = min_left.min(w.left);
            max_right = max_right.max(w.left + w.width);
            min_top = min_top.min(w.top);
            max_bottom = max_bottom.max(w.top + w.height);
        }
    }

    LayoutHint {
        class_name: LayoutHintClass::Table,
        confidence: SYNTHETIC_HINT_CONFIDENCE,
        left: min_left as f32,
        right: max_right as f32,
        top: page_height - min_top as f32,
        bottom: page_height - max_bottom as f32,
    }
}

/// Group filtered words into visual rows by top coordinate. `indexed` holds the
/// non-empty word indices; the returned rows preserve those indices.
fn group_rows(words: &[HocrWord], indexed: &[usize], median_height: u32) -> Vec<Row> {
    let mut order: Vec<usize> = indexed.to_vec();
    order.sort_by_key(|&i| (words[i].top, words[i].left));

    let tolerance = ((median_height as f32 * ROW_GROUPING_FACTOR).round() as u32).max(2);
    let mut rows: Vec<Row> = Vec::new();
    for &i in &order {
        match rows.last_mut() {
            Some(row) if words[i].top.saturating_sub(row.top) <= tolerance => {
                row.word_indices.push(i);
            }
            _ => rows.push(Row {
                word_indices: vec![i],
                top: words[i].top,
            }),
        }
    }
    rows
}

fn median_word_height(words: &[HocrWord], indexed: &[usize]) -> u32 {
    let mut heights: Vec<u32> = indexed.iter().map(|&i| words[i].height).collect();
    heights.sort_unstable();
    heights.get(heights.len() / 2).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, left: u32, top: u32, width: u32) -> HocrWord {
        HocrWord {
            text: text.to_string(),
            left,
            top,
            width,
            height: 10,
            confidence: 95.0,
        }
    }

    /// The #1316 reproducer shape: a 5-column, header + 2-data-row borderless
    /// numeric grid. The detector must propose one Table region spanning it.
    #[test]
    fn detects_sparse_five_column_borderless_grid() {
        let xs = [40u32, 320, 400, 470, 540];
        let headers = ["Description", "Quantity", "Unit", "VAT", "Total"];
        let mut words = Vec::new();
        for (x, h) in xs.iter().zip(headers) {
            words.push(word(h, *x, 100, 60));
        }
        for (row, vals) in [
            ["PROD_1", "1", "10.00", "19%", "10.00"],
            ["PROD_2", "2", "20.00", "19%", "40.00"],
        ]
        .iter()
        .enumerate()
        {
            let y = 120 + row as u32 * 15;
            for (x, v) in xs.iter().zip(vals) {
                words.push(word(v, *x, y, 40));
            }
        }

        let hints = detect_geometric_table_hints(&words, 842.0);
        assert_eq!(hints.len(), 1, "expected exactly one synthesized table region");
        let hint = &hints[0];
        assert_eq!(hint.class_name, LayoutHintClass::Table);
        // Region must be in PDF coords with top above bottom and cover the grid.
        assert!(hint.top > hint.bottom, "PDF top must exceed bottom");
        assert!(hint.left <= 40.0 && hint.right >= 580.0, "region must span the columns");
    }

    /// Two-column prose reflow (references, body text) has only two column
    /// anchors and must NOT be proposed — the ≥3-column guard rejects it before
    /// it can reach the downstream reconstruction guards.
    #[test]
    fn rejects_two_column_prose() {
        let mut words = Vec::new();
        for row in 0..5u32 {
            let y = 100 + row * 14;
            words.push(word("some", 40, y, 80));
            words.push(word("phrase", 300, y, 90));
        }
        let hints = detect_geometric_table_hints(&words, 842.0);
        assert!(hints.is_empty(), "2-column prose must not be proposed as a table");
    }

    /// An alphabetic 3-column grid (e.g. a word/name list) is NOT proposed:
    /// the numeric-dominance gate reserves this fallback for numeric borderless
    /// tables and leaves textual tables to the ML detector (#1316 scope).
    #[test]
    fn rejects_alphabetic_three_column_grid() {
        let xs = [40u32, 200, 360];
        let mut words = Vec::new();
        for row in 0..4u32 {
            let y = 100 + row * 15;
            for (c, x) in xs.iter().enumerate() {
                words.push(word(&format!("word{row}{c}"), *x, y, 70));
            }
        }
        let hints = detect_geometric_table_hints(&words, 842.0);
        assert!(hints.is_empty(), "alphabetic grid must not be proposed (numeric gate)");
    }

    /// An equation-dense grid whose "numbers" are math variables and subscripts
    /// (`a1`, `2j`, `j0`, `γδ`) must NOT be proposed: `is_numeric_token` rejects
    /// any letter-bearing token, so the run fails the numeric-dominance gate.
    /// Regression guard for the 1304.6413 academic-paper false positive (#1316).
    #[test]
    fn rejects_equation_subscript_grid() {
        let xs = [40u32, 200, 360, 520];
        let cells = [
            ["a1", "2j", "j0", "γδ"],
            ["b1", "2b", "t1", "wn"],
            ["x1", "y1", "2a", "nx2"],
        ];
        let mut words = Vec::new();
        for (row, vals) in cells.iter().enumerate() {
            let y = 100 + row as u32 * 15;
            for (x, v) in xs.iter().zip(vals) {
                words.push(word(v, *x, y, 40));
            }
        }
        let hints = detect_geometric_table_hints(&words, 842.0);
        assert!(
            hints.is_empty(),
            "equation-subscript grid must not be proposed (numeric gate)"
        );
    }

    /// A single tabular row (no vertical repetition) is not a table.
    #[test]
    fn rejects_single_tabular_row() {
        let words = vec![
            word("A", 40, 100, 30),
            word("B", 200, 100, 30),
            word("C", 400, 100, 30),
            word("prose", 40, 200, 300),
        ];
        let hints = detect_geometric_table_hints(&words, 842.0);
        assert!(hints.is_empty(), "a lone tabular row must not be proposed");
    }

    /// The `XBERG_LAYOUT_NO_GEOMETRIC_TABLES` A/B toggle short-circuits detection.
    /// (Env-driven; asserted indirectly via the empty-input fast path staying
    /// empty — the flag is exercised by the benchmark A/B, not unit state.)
    #[test]
    fn empty_input_returns_no_hints() {
        assert!(detect_geometric_table_hints(&[], 842.0).is_empty());
    }
}
