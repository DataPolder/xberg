//! XY-Cut recursive spatial partitioning for multi-column text layout.
//!
//! This module implements the XY-Cut algorithm per PDF Spec Section 9.4 for
//! recursive geometric analysis without semantic heuristics. Uses projection
//! profiles to detect column boundaries in complex layouts.
//!
//! Per ISO 32000-1:2008:
//! - Section 9.4: Text Objects and coordinates
//! - Section 14.7: Logical Structure (prefers structure tree when available)
//!
//! # Algorithm Overview
//!
//! 1. Compute horizontal projection (white space density across X)
//! 2. Find valleys (gaps) where density < threshold
//! 3. Split region at widest valley (vertical line)
//! 4. Recursively partition left and right sub-regions
//! 5. Alternate to vertical projection if no horizontal valleys found
//! 6. Base case: Sort spans top-to-bottom, left-to-right
//!
//! # Performance
//!
//! Typical newspaper page: ~100 spans, < 5ms processing time
//! Recursive depth: O(log n) for balanced columns

// TODO(xberg-io/xberg#1567): 4 cyclomatic-complexity and 13 size/complexity findings
// in this file, currently excluded via the quality-debt baseline in alef.toml. Splitting
// these needs compiler-in-the-loop verification, not a mechanical pass. Delete this
// note and the file's baseline entry together once it goes green. Help wanted.

use super::{ReadingOrderContext, ReadingOrderStrategy};
use crate::error::Result;
use crate::geometry::Rect;
use crate::layout::TextSpan;
use crate::pipeline::{OrderedTextSpan, ReadingOrderInfo};

/// Maximum density-array length for XY-cut projection profiles.
///
/// A normal PDF page is at most a few thousand points wide/tall. This limit of
/// 100 000 bins is generous (≈ 33× a 3000-point A0 page) while being small
/// enough to never cause an allocation problem. Spans whose bounding-box span
/// exceeds this limit are the result of a degenerate CTM; returning `None` from
/// the projection safely skips the split instead of attempting a multi-terabyte
/// allocation that would abort the process via `handle_alloc_error`.
const MAX_PROJECTION_SIZE: usize = 100_000;

/// Coarse classification of a region for the multi-column-prose
/// fix. Used to gate the tight-gutter cut: tight cuts are only accepted on
/// regions that *positively* identify as prose, so the same XY-cut recursion
/// no longer corrupts table cells (the lesson — see lines 73–101).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKind {
    /// Tall stack of wide lines OR tall stack of half-column lines with
    /// substantial content per line. Safe to apply tight-gutter cuts.
    Prose,
    /// Short cells in a grid (mean characters per line < 8). Tight cuts
    /// here corrupt cell ordering — the canonical google_doc population
    /// table that reverted two earlier attempts is the prototype.
    Table,
    /// Anything else — too few lines, mixed shapes, decorative regions.
    /// Default to the behaviour (no tight cut).
    Mixed,
}

/// Contiguous run of bold-or-larger-font spans spanning ≥ 2 visual lines
/// that the XY-cut splitter must treat as an atomic block. Built by
/// `find_heading_runs` BEFORE recursive partitioning,
/// then substituted into the partition input as a single wide synthetic
/// span so cluster-detection / valley-finding can't drive a vertical
/// cut THROUGH a wrapped heading.
///
/// After partition completes, `expand_blocks` projects the synthetic
/// placeholder back into its constituent original spans, preserving
/// each span's per-glyph metadata for downstream consumers
/// (markdown converter heading-level inference, layout-preserving
/// DOCX export, etc.).
#[derive(Debug, Clone)]
struct HeadingRun {
    /// Indices into the original `&[TextSpan]` slice, in reading order
    /// (top-to-bottom, left-to-right within a line).
    span_indices: Vec<usize>,
    /// Union of the constituent spans' bboxes. Substituted for each
    /// individual bbox during partition so the heading appears as one
    /// wide bbox.
    combined_bbox: Rect,
}

/// Within a valley run `[start, end)` of a projection profile's density
/// array, choose the split OFFSET at the run's deepest (lowest-density)
/// point rather than its arithmetic midpoint (GH#1763).
///
/// A wide interior run classified "below threshold" is not uniformly
/// empty: `horizontal_projection_indexed` only excludes spans wider than
/// 55% of the region and spans with fewer than 2 non-whitespace
/// characters, so a full-width caption line or a single-char table-cell
/// row can still occupy bins inside the run with nonzero (but
/// sub-threshold) density. The run's arithmetic midpoint has no relation
/// to where that content sits, so it can land squarely inside a figure
/// caption even though a genuinely empty gutter exists elsewhere in the
/// same run.
///
/// Tie-break order, applied to the contiguous sub-runs that attain the
/// run's minimum density value:
///   1. Widest sub-run wins — a genuine open gutter is wide; an isolated
///      single-bin dip that happens to share the same minimum density
///      is not a gutter and should not win over a real one.
///   2. On a width tie, the sub-run whose center is nearest the WHOLE
///      run's arithmetic midpoint wins — keeps the choice deterministic
///      and, when nothing else distinguishes the candidates, close to
///      the pre-fix behavior.
///
/// When the run is uniformly at its minimum density throughout (the
/// common case: a real, empty column gutter with no stray content), the
/// single minimal sub-run IS the whole run, so this returns exactly the
/// old midpoint — the fix only changes behavior in the buggy case where
/// sub-threshold content is unevenly distributed inside the run. That
/// exactness is why centers are computed in f32 as `(lo + hi) / 2`:
/// an integer `lo + width / 2` truncates, which would shift the split
/// by half a unit on every odd-width run. That arithmetic is pinned by
/// `a_candidate_that_would_cut_a_span_is_rejected`, whose chosen gap has
/// odd width -- NOT by `uniform_run_split_is_unchanged_from_the_legacy_midpoint`,
/// which cannot reach it: a uniform run's midpoint is by definition at the
/// floor, so the guard below returns before any centre is computed. ~keep
/// Share of the projected region a below-threshold run must exceed before its midpoint is
/// treated as untrustworthy (GH#1763).
///
/// A real column gutter is narrow -- 15 to 80 pt on a region of roughly 500 pt, so 3% to 16%
/// -- and its midpoint is the gutter, which is why splitting there worked for years. The
/// GH#1763 page is the opposite case: its "valley" is 254 pt of a 523 pt region, 48.6%,
/// because the left column is a figure and a 7.2 pt caption that fall below the density
/// threshold almost everywhere. A run that wide is not a gutter at all, it is a sparse
/// region, and its arithmetic midpoint says nothing about where the columns divide.
///
/// 35% sits well above any plausible gutter and well below the reporting page. Relocating
/// regardless of run width was measured across 230 corpus documents and was not an
/// improvement: 23 documents changed and, by absolute dictionary-valid word count, more got
/// worse than better. ~keep
const SPARSE_VALLEY_REGION_SHARE: f32 = 0.35;

fn deepest_valley_point(density: &[f32], start: usize, end: usize, split_is_clear: &dyn Fn(f32) -> bool) -> f32 {
    debug_assert!(start < end && end <= density.len());
    if start >= end || end > density.len() {
        return (start + end) as f32 / 2.0;
    }
    let run_mid = (start + end) as f32 / 2.0;
    let min_density = density[start..end].iter().copied().fold(f32::INFINITY, f32::min);

    // A narrow run IS the gutter, and its midpoint is the right place to split; only a run
    // too wide to be a gutter has an untrustworthy midpoint. See SPARSE_VALLEY_REGION_SHARE. ~keep
    if ((end - start) as f32) <= density.len() as f32 * SPARSE_VALLEY_REGION_SHARE {
        return (start + end) as f32 / 2.0;
    }

    // Relocate ONLY when the midpoint actually lands on content -- the defect's own
    // precondition. If the midpoint already sits at the run's density floor, the split is
    // already falling through empty space and cutting nothing, so moving it to some wider
    // empty region elsewhere changes reading order for no benefit. Measured: without this
    // guard the fix altered 23 of 230 corpus documents and, counted by dictionary-valid
    // words, made 11 worse against 8 better -- the split was being relocated on pages that
    // had nothing wrong with them. A split line of odd width falls between two bins; either
    // one being at the floor is enough to leave it alone. ~keep
    let mid_low = run_mid.floor() as usize;
    let mid_high = (run_mid.ceil() as usize).min(end - 1);
    if density[mid_low] == min_density || density[mid_high] == min_density {
        return run_mid;
    }

    let mut candidates: Vec<(usize, f32)> = Vec::new();
    let mut i = start;
    while i < end {
        if density[i] != min_density {
            i += 1;
            continue;
        }
        let sub_start = i;
        while i < end && density[i] == min_density {
            i += 1;
        }
        candidates.push((i - sub_start, (sub_start + i) as f32 / 2.0));
    }

    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| crate::utils::safe_float_cmp((left.1 - run_mid).abs(), (right.1 - run_mid).abs()))
    });

    // A zero in this profile does NOT mean no glyphs: `horizontal_projection_indexed`
    // deliberately omits spans wider than 55% of the region, spans of fewer than two
    // non-whitespace characters, and the part of every span beyond its estimated text core.
    // Seeking the deepest point therefore steers the split straight at the regions those
    // omissions create, which is the opposite of what a midpoint did by accident. Measured
    // without this check: 22 of 230 corpus documents changed and words came apart at
    // single-character spans -- "virgin" into "v" + "irgin", "test" into "t" + "est" --
    // because the split landed between two spans of one word. Candidates are therefore
    // checked against the real span extents, not the profile, and a run whose candidates all
    // cut something keeps the midpoint, which is exactly the pre-GH#1763 behaviour. ~keep
    for (_, center) in candidates {
        if split_is_clear(center) {
            return center;
        }
    }
    run_mid
}

/// Union of the bboxes of `spans[indices]`. Empty index list yields a
/// zero-sized rect at the origin (never built in practice — guarded by
/// the caller).
fn union_bboxes(spans: &[TextSpan], indices: &[usize]) -> Rect {
    let mut x_min = f32::MAX;
    let mut y_min = f32::MAX;
    let mut x_max = f32::MIN;
    let mut y_max = f32::MIN;
    for &i in indices {
        let b = spans[i].bbox;
        x_min = x_min.min(b.left());
        x_max = x_max.max(b.right());
        y_min = y_min.min(b.top());
        y_max = y_max.max(b.bottom());
    }
    if x_min == f32::MAX {
        return Rect::default();
    }
    Rect::from_points(x_min, y_min, x_max, y_max)
}

/// XY-Cut recursive spatial partitioning strategy.
///
/// Detects columns using projection profiles and white space analysis.
/// Suitable for newspapers, academic papers, and multi-column layouts.
pub struct XYCutStrategy {
    /// Minimum number of spans in a region before attempting split (default: 5).
    /// Prevents excessive recursion on small regions.
    pub min_spans_for_split: usize,

    /// Valley threshold as fraction of peak projection density (default: 0.3).
    /// Lower values detect narrower gutters, higher values only detect wide gaps.
    pub valley_threshold: f32,

    /// Minimum valley width in points (default: 15.0).
    /// Prevents detecting single-character gaps as column boundaries.
    pub min_valley_width: f32,

    /// Enable horizontal partitioning first, fallback to vertical (default: true).
    ///
    /// Per PDF Spec ISO 32000-1:2008 §14.8.4 (Logical Structure reading order),
    /// column detection is the primary purpose of XY-Cut — horizontal-first
    /// (vertical cut line) splits columns before rows, matching Western
    /// top-down-left-to-right reading order in multi-column documents.
    /// Callers with row-dominant layouts can override via
    /// `with_prefer_horizontal(false)`.
    pub prefer_horizontal: bool,
}

/// Cap on `partition_indexed` recursion depth. Real layouts nest only a few
/// splits deep; this bound only fires on the singleton-peel pathology (many
/// distinct-Y header/footer strips) where unbounded depth is O(n² log n). Set
/// high enough that no real document reaches it.
const MAX_PARTITION_DEPTH: u32 = 64;

// GH#1808: visual-line y-tolerance for the full-width-line peel and the whole-line
// partition assignment. Matches `find_heading_runs`'s own `same_line` tolerance rather than
// introducing a second answer to "are these two spans on the same line".
const XYCUT_LINE_Y_TOLERANCE_PTS: f32 = 1.0;
// A line whose inked extent reaches this fraction of the region's own width is full-width
// furniture (a legend, a running header, a caption) rather than column content -- no real
// two-column body line's own width can reach here, since a column is bounded by the page
// margin AND the gutter (never much past ~48% of the region on a normal two-column split).
const FULL_WIDTH_LINE_FRACTION: f32 = 0.90;
// A single full-width line is an ordinary title or footer -- already handled by
// `is_single_column_region`'s bridge exclusion elsewhere -- and peeling it alone would cost
// a recursive call for no benefit. The population this fix targets starts at multi-line
// legends and table notes (GH#1808's own reproducer is 8 lines).
const MIN_PEELED_FULL_WIDTH_LINES: usize = 2;
// The gap, in ems of the line's own max font size, at which a line stops being "continuous"
// across a column cut. A row's two cells are separated by the gutter (several ems); a
// legend's own font runs abut at normal word/kern spacing.
const MAX_INTRA_LINE_GAP_EM: f32 = 1.0;
// the smaller side of a line must hold at least this share of its ink for the line to
// count as crossing a column cut (and be assigned by its majority, see `partition_lines_at`).
// A legend through the gutter splits near evenly (upstream's own straddle test is 42/58); a
// body line cut a third of the way into its own column (29/71) is not crossing anything.
const MIN_CROSSING_SHARE: f32 = 0.4;
// a line this many ems of ink wide or less, first on its row, is a lead -- a hanging
// number, a bullet, a label -- that travels with the line after it (see `partition_lines_at`).
const MAX_LEAD_EM: f32 = 4.0;
// This many lines crossing a column cut (`MIN_CROSSING_SHARE` of their ink on each
// side), with nothing else beside them on the cut's right, make a band of prose the cut runs
// through rather than a gutter -- see `prose_band_split`.
const MIN_CROSSING_LINES: usize = 3;

/// The band of prose `split_x` runs through, if it runs through one, with what lies
/// above and below it: `[above, band, below]` in reading order, empty parts left out.
///
/// The band is the y-extent of the continuous lines (`group_indices_into_rows`) that hold
/// at least `MIN_CROSSING_SHARE` of their ink on EACH side of the cut; it takes
/// `MIN_CROSSING_LINES` of them, and NOTHING else may start right of the cut within that
/// extent. That last condition separates a list above a table (the table's columns sit
/// below the band) from a two-column page with a wide table, title or caption: there the
/// other column runs beside the crossing lines, and the cut is its gutter. A band of wide
/// quotes under two columns qualifies too -- the columns above it are then cut on their
/// own, which is why the band is peeled rather than the cut refused. ~keep
fn prose_band_split(all_spans: &[TextSpan], indices: &[usize], split_x: f32) -> Option<Vec<Vec<usize>>> {
    let extent = |i: usize| {
        let bbox = &all_spans[i].bbox;
        (bbox.bottom().min(bbox.top()), bbox.bottom().max(bbox.top()))
    };
    let mut crossing: Vec<usize> = Vec::new();
    let mut crossing_lines = 0usize;
    for line in group_indices_into_rows(all_spans, indices).iter().flatten() {
        let mut left_width = 0.0f32;
        let mut right_width = 0.0f32;
        for &i in line {
            let left = all_spans[i].bbox.left();
            let right = ink_right(&all_spans[i]);
            left_width += (right.min(split_x) - left).max(0.0);
            right_width += (right - left.max(split_x)).max(0.0);
        }
        let ink = left_width + right_width;
        if ink > 0.0 && left_width.min(right_width) >= MIN_CROSSING_SHARE * ink {
            crossing.extend(line);
            crossing_lines += 1;
        }
    }
    if crossing_lines < MIN_CROSSING_LINES {
        return None;
    }
    let band_bottom = crossing.iter().map(|&i| extent(i).0).fold(f32::MAX, f32::min);
    let band_top = crossing.iter().map(|&i| extent(i).1).fold(f32::MIN, f32::max);
    let in_band = |i: usize| {
        let (lo, hi) = extent(i);
        lo < band_top && hi > band_bottom
    };
    let crowded = indices
        .iter()
        .any(|&i| in_band(i) && all_spans[i].bbox.left() >= split_x && !crossing.contains(&i));
    if crowded {
        return None;
    }
    let mut above = Vec::new();
    let mut band = Vec::new();
    let mut below = Vec::new();
    for &i in indices {
        if in_band(i) {
            band.push(i);
        } else if extent(i).0 >= band_top {
            above.push(i);
        } else {
            below.push(i);
        }
    }
    Some(
        [above, band, below]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect(),
    )
}

/// the right edge of a span's INK, for the whole-line machinery below. An extractor bbox
/// can reach far past its glyphs (trailing whitespace, a stretched advance width -- a
/// running header `J.S. Levine et al.` measured 465 pt wide), and read raw it fuses a
/// left-column line with the right column's line on the same baseline, counts the pair as
/// full width and sends it to one side whole. Clamp the bbox to a generous upper bound on
/// what its text can occupy: 0.75 em per visible character and 1.5 em per space, well
/// above any real glyph run (body text averages ~0.5 em), so a genuine span keeps its
/// bbox and only an inflated one is cut back. ~keep
fn ink_right(span: &TextSpan) -> f32 {
    let left = span.bbox.left();
    let right = span.bbox.right();
    let em = span.font_size.max(1.0);
    // Trailing whitespace is padding, not ink: the header above is 18 characters and ~80
    // trailing spaces, and counting those spaces would let its bbox stand.
    let text = span.text.trim_end();
    let visible = text.chars().filter(|c| !c.is_whitespace()).count() as f32;
    let spaces = text.chars().filter(|c| c.is_whitespace()).count() as f32;
    if visible == 0.0 {
        return right;
    }
    right.min(left + (visible * 0.75 + spaces * 1.5) * em)
}

/// Group `indices` into visual lines: anchored (not chained) on `y`, top to bottom, and
/// further split on `x` wherever consecutive items are more than `MAX_INTRA_LINE_GAP_EM`
/// ems apart -- so two unrelated fragments that merely share a baseline (a header printed
/// once per column, at the same `y` but 50pt apart) are never treated as one line. A line
/// returned by this function is therefore a genuinely CONTINUOUS run of ink, which is what
/// both `peel_full_width_line_bands` and `partition_lines_at` need "line" to mean. ~keep
// the peel and the partition both work per row now; the flat view is kept for the tests.
#[cfg(test)]
fn group_indices_into_lines(all_spans: &[TextSpan], indices: &[usize]) -> Vec<Vec<usize>> {
    group_indices_into_rows(all_spans, indices)
        .into_iter()
        .flatten()
        .collect()
}

/// the same grouping, kept per visual ROW -- every line that shares one `y` bucket. The
/// peel needs the row: a numbered clause set with a hanging number (`24.1` at x 71, its
/// text from x 110, more than an em apart) is two lines on one row, and peeling only the
/// full-width text line strands the number in another band, where it stops being the
/// heading's number. ~keep
fn group_indices_into_rows(all_spans: &[TextSpan], indices: &[usize]) -> Vec<Vec<Vec<usize>>> {
    let mut order: Vec<usize> = indices.to_vec();
    order.sort_by(|&a, &b| {
        all_spans[b]
            .bbox
            .y
            .total_cmp(&all_spans[a].bbox.y)
            .then_with(|| all_spans[a].bbox.left().total_cmp(&all_spans[b].bbox.left()))
    });
    let mut y_buckets: Vec<Vec<usize>> = Vec::new();
    let mut anchor_y = f32::NAN;
    for index in order {
        let y = all_spans[index].bbox.y;
        if y_buckets.is_empty() || (anchor_y - y).abs() > XYCUT_LINE_Y_TOLERANCE_PTS {
            anchor_y = y;
            y_buckets.push(Vec::new());
        }
        y_buckets.last_mut().expect("just pushed above").push(index);
    }
    let mut rows: Vec<Vec<Vec<usize>>> = Vec::new();
    for mut bucket in y_buckets {
        let mut lines: Vec<Vec<usize>> = Vec::new();
        // The global sort above orders by `y` first and uses `x` only to break an EXACT tie,
        // so inside a bucket (spans within `XYCUT_LINE_Y_TOLERANCE_PTS` of each other) the
        // order is still `y` order: a right-column table cell set a fraction of a point above
        // a left-column body line comes FIRST. Measured left to right from there, the "gap"
        // to the body line is hundreds of points NEGATIVE -- never more than an em -- and the
        // two columns are fused into one full-width line, which the peel then lifts out as a
        // band and `partition_lines_at` assigns whole. Order each bucket by `x` before
        // splitting it, and measure each gap from the run's rightmost ink so far, not from
        // its last span, so an overlapping pair cannot hide a gutter behind it. ~keep
        bucket.sort_by(|&a, &b| all_spans[a].bbox.left().total_cmp(&all_spans[b].bbox.left()));
        let mut run: Vec<usize> = Vec::new();
        let mut run_right = f32::MIN;
        for index in bucket {
            if let Some(&last) = run.last() {
                let gap = all_spans[index].bbox.left() - run_right;
                let max_font = all_spans[last].font_size.max(all_spans[index].font_size);
                if gap > max_font * MAX_INTRA_LINE_GAP_EM {
                    lines.push(std::mem::take(&mut run));
                    run_right = f32::MIN;
                }
            }
            run_right = run_right.max(ink_right(&all_spans[index]));
            run.push(index);
        }
        if !run.is_empty() {
            lines.push(run);
        }
        rows.push(lines);
    }
    rows
}

// How far from the page's own column gutter (`ReadingOrderContext::column_gutter`, from
// `PdfDocument::detect_column_gutter`) a fallback valley's cut may lie. ~keep
const FALLBACK_GUTTER_TOLERANCE_PT: f32 = 24.0;

thread_local! {
    /// The page gutter of the `apply` / `partition_region` call in progress, for the
    /// valley fallback deep in the recursion. ~keep
    static PAGE_GUTTER: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
}

/// Sets [`PAGE_GUTTER`] for one ordering call and restores the previous value. ~keep
struct PageGutterScope(Option<f32>);

impl PageGutterScope {
    fn set(gutter: Option<f32>) -> Self {
        Self(PAGE_GUTTER.with(|cell| cell.replace(gutter)))
    }
}

impl Drop for PageGutterScope {
    fn drop(&mut self) {
        PAGE_GUTTER.with(|cell| cell.set(self.0));
    }
}

/// Both sides of a cut at `split_x` are at least a column wide (60 pt, the same floor as
/// [`XYCutStrategy::column_cut_at`]'s own check, which it repeats). ~keep
fn leaves_two_columns(all_spans: &[TextSpan], indices: &[usize], split_x: f32) -> bool {
    const MIN_RESULT_WIDTH_PT: f32 = 60.0;
    let (mut left_min, mut left_max, mut right_min, mut right_max) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &i in indices {
        let (l, r) = (all_spans[i].bbox.left(), all_spans[i].bbox.right());
        if l < split_x {
            left_min = left_min.min(l);
            left_max = left_max.max(r);
        } else {
            right_min = right_min.min(l);
            right_max = right_max.max(r);
        }
    }
    left_max - left_min >= MIN_RESULT_WIDTH_PT && right_max - right_min >= MIN_RESULT_WIDTH_PT
}

impl Default for XYCutStrategy {
    fn default() -> Self {
        Self {
            min_spans_for_split: 5,
            valley_threshold: 0.3,
            // 15pt. A fix for multi-column prose interleaving was attempted
            // TWICE and REVERTED both times — the 70-PDF sweep caught data
            // corruption in the google_doc population table's digits
            // ("273.879.7501" -> "1273.879.750") each time:
            //
            //   Attempt 1 — lower min_valley_width 15 -> 12 so the tight
            //   ~12pt two-column gutter is detected. Also split the
            //   table's ~12pt inter-cell gaps -> reordered digits.
            //
            //   Attempt 2 — a structural find_two_column_prose_split
            //   (exactly-two recurring left-edge clusters, wide columns,
            //   clean gutter) tried before the single-column check. It
            //   never fired on the target page's WHOLE extent (three left-edge
            //   clusters: full-width intro/footer @60 + left @82 + right
            //   @312, because is_single_column blocks band separation
            //   first), yet it DID fire on a 2-column sub-region of the
            //   google_doc table and reordered cells.
            //
            // Root cause: the same XY-Cut machinery orders both
            // prose-columns and table-cells. Any sensitivity increase
            // that catches tight 2-column prose also splits
            // table cells and corrupts data. A correct fix needs a
            // real table-vs-prose classifier (column cells are short
            // values; prose columns are tall stacks of wide lines) AND
            // recursive band-separation of full-width header/footer rows
            // before column detection — a substantial XY-Cut redesign,
            // validated against the full CI corpus, not a local tweak. ~keep
            min_valley_width: 15.0,
            prefer_horizontal: true,
        }
    }
}

impl XYCutStrategy {
    /// Create a new XY-Cut strategy with default parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom valley threshold (0.0-1.0).
    pub fn with_valley_threshold(mut self, threshold: f32) -> Self {
        self.valley_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Create with custom minimum valley width.
    pub fn with_min_valley_width(mut self, width: f32) -> Self {
        self.min_valley_width = width.max(1.0);
        self
    }

    /// Enable or disable horizontal partitioning first preference.
    pub fn with_prefer_horizontal(mut self, prefer: bool) -> Self {
        self.prefer_horizontal = prefer;
        self
    }

    /// Core recursive partitioning algorithm.
    ///
    /// Public for use by MarkdownConverter's ColumnAware reading order mode.
    ///
    /// Runs a pre-pass that detects multi-line heading runs
    /// (bold or larger-than-body font, ≥ 2 wrapped lines with matching
    /// X-extent) and locks them as atomic blocks the recursive splitter
    /// cannot split. Without this, a wrapped heading whose tail lines
    /// Y-overlap with adjacent-column dense content (table caption, table
    /// row, image label) gets bucketed across columns: line 1 glued to the
    /// body paragraph, line 2..N orphaned into the wrong block — and the
    /// markdown converter then promotes the orphan tail to a phantom
    /// heading (`### …`) in the wrong location.
    ///
    /// `column_gutter` is the mid-X of the page's column gutter when the
    /// caller has detected one; it keeps the heading-run pre-pass from
    /// folding a heading that opens the other column into a wrapped
    /// heading's run (GH#1757). `None` leaves that fold unconditional.
    pub fn partition_region(&self, spans: &[TextSpan], column_gutter: Option<f32>) -> Vec<Vec<TextSpan>> {
        let _gutter = PageGutterScope::set(column_gutter);
        let heading_runs = self.find_heading_runs(spans, column_gutter);
        if heading_runs.is_empty() {
            // Hot path: no headings found, skip the synthesize/expand
            // pair entirely so the cost is bounded to one O(n log n) sort
            // inside find_heading_runs. ~keep
            let indices: Vec<usize> = (0..spans.len()).collect();
            let index_groups = self.partition_indexed(spans, &indices);
            return index_groups
                .into_iter()
                .map(|group| group.into_iter().map(|i| spans[i].clone()).collect())
                .collect();
        }

        let (synthetic, synthetic_origin) = self.synthesize_for_partition(spans, &heading_runs);
        let synth_indices: Vec<usize> = (0..synthetic.len()).collect();
        let synth_groups = self.partition_indexed(&synthetic, &synth_indices);

        self.expand_blocks(synth_groups, spans, &synthetic_origin)
    }

    /// Detect contiguous bold/large-font runs that span ≥ 2 lines with
    /// matching X-extent (i.e. wrapped subsection headings).
    ///
    /// Per the fix-543 plan §A.2: two adjacent spans (in reading
    /// order) are considered to belong to the same heading run when
    /// ALL of the following hold:
    ///
    /// 1. Both are heading-like (bold, OR font_size > median × 1.15).
    /// 2. Same font_size (within 0.5 pt epsilon).
    /// 3. Same bold flag.
    /// 4. Next span's left edge is within `[prev.left, prev.left + 6pt]`
    ///    (wrapped heading lines often re-indent by up to ~6pt).
    /// 5. Next span sits ≤ 1.5 × line-height below the previous span
    ///    (a single-line gap; double-line gaps are paragraph breaks).
    ///
    /// `median_font_size` is computed across non-bold spans so heavy
    /// bold runs don't bias the body-size estimate upward.
    fn find_heading_runs(&self, spans: &[TextSpan], column_gutter: Option<f32>) -> Vec<HeadingRun> {
        if spans.len() < 2 {
            return Vec::new();
        }

        // Median body font size from NON-bold spans only. Bold spans
        // typically sit at heading sizes (bigger than body), so including
        // them biases the median high and we'd miss bold headings whose
        // size sits between body and the heavier weight tier. ~keep
        let mut non_bold_sizes: Vec<f32> = spans
            .iter()
            .filter(|s| !s.font_weight.is_bold())
            .map(|s| s.font_size)
            .filter(|&sz| sz > 0.0)
            .collect();
        let median_body = if non_bold_sizes.is_empty() {
            let mut sizes: Vec<f32> = spans.iter().map(|s| s.font_size).filter(|&sz| sz > 0.0).collect();
            if sizes.is_empty() {
                return Vec::new();
            }
            sizes.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            sizes[sizes.len() / 2]
        } else {
            non_bold_sizes.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            non_bold_sizes[non_bold_sizes.len() / 2]
        };
        let heading_size_floor = median_body * 1.15;

        let is_heading_like = |s: &TextSpan| -> bool { s.font_weight.is_bold() || s.font_size > heading_size_floor };

        // Sort indices by reading order (top of page first; Rect::top()
        // is the SMALLER Y of the normalized rect — see comment at
        // line ~885 — so larger Y = higher on page in PDF coords;
        // we want DESCENDING Y here). ~keep
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by(|&a, &b| {
            let y_cmp = crate::utils::safe_float_cmp(spans[b].bbox.top(), spans[a].bbox.top());
            if y_cmp != std::cmp::Ordering::Equal {
                return y_cmp;
            }
            crate::utils::safe_float_cmp(spans[a].bbox.left(), spans[b].bbox.left())
        });

        // Cluster reading-order-adjacent heading-like spans into runs.
        // The same line may carry multiple bold spans (one per Tj
        // segment); we collapse runs across lines, not within a line. ~keep
        let indent_tolerance = 6.0_f32;
        let font_eps = 0.5_f32;
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();

        for &idx in &order {
            let span = &spans[idx];

            // A whitespace-only span carries no ink, so it must not break an
            // otherwise-continuous heading run. A `TJ` array's tab kern
            // (`[(3.)-1329.5(Title)] TJ`) turns into exactly such a span,
            // always `FontWeight::Normal` regardless of the surrounding
            // bold context (see `advance.rs`'s synthetic-space
            // construction) — so `is_heading_like` below would reject it
            // and sever the run at the marker/title boundary, splitting a
            // numbered heading's marker from its own title into two
            // separate (non-heading, single-span) fragments while the
            // wrapped continuation line still merges into one. The
            // narrower fragment's union bbox then no longer covers the
            // marker's column position, letting an unrelated span at the
            // marker's row/column slot in between during partitioning.
            // Skipping (not breaking, not joining) the whitespace span
            // keeps the run open across it, exactly as the same producer
            // setting the marker in a separate text object (no kern, no
            // synthetic space) already does. ~keep
            if span.text.chars().all(char::is_whitespace) {
                continue;
            }

            if !is_heading_like(span) {
                if !current.is_empty() {
                    runs.push(std::mem::take(&mut current));
                }
                continue;
            }

            if current.is_empty() {
                current.push(idx);
                continue;
            }

            let last_idx = *current.last().unwrap();
            let last = &spans[last_idx];

            let size_ok = (span.font_size - last.font_size).abs() <= font_eps;
            let bold_ok = span.font_weight.is_bold() == last.font_weight.is_bold();

            // Same-line: top within 1 pt of last's top — fold without
            // applying indent/leading checks (both spans belong to the
            // SAME wrapped-heading line, e.g. two bold Tj segments). ~keep
            let same_line = (span.bbox.top() - last.bbox.top()).abs() <= 1.0;

            // Rows are sorted by top across the WHOLE page, so a heading
            // opening the other column can sort between a wrapped heading's
            // two lines (GH#1757: 0.25 pt below line 1, 78.9 pt away across
            // the gutter). Skip it — neither fold it in nor let it close the
            // run — so the run stays open for the real continuation line,
            // which is the whole point of this pre-pass. Folding it in makes
            // it the run's last span and the continuation line then fails the
            // indent test against the WRONG column's x; letting it break the
            // run leaves two single-line candidates that the >= 2 distinct
            // lines filter below drops. This sits BEFORE the size/weight
            // tests because both failure modes cost the run: the issue's
            // 10 pt variant fails `size_ok` and breaks it instead.
            //
            // Inert unless the caller supplied a gutter, so every XY-cut
            // entry point on an output path must pass one — see
            // `PdfDocument::detect_column_gutter`. ~keep
            if same_line && Self::same_line_span_belongs_to_other_column(span, last, column_gutter) {
                continue;
            }

            if size_ok && bold_ok && same_line {
                current.push(idx);
                continue;
            }

            // Different line: enforce indent (4) + leading (5).
            // line_height = max of the two spans' bbox heights, plus a
            // floor of font_size to handle ascender-only / descender-only
            // glyphs with collapsed bboxes. ~keep
            let line_h = last.bbox.height.max(span.bbox.height).max(last.font_size).max(1.0);
            let leading_tolerance = line_h * 1.5;

            // PDF coords: y grows up, so the wrapped line sits at a
            // SMALLER bbox.top than the previous line. The gap between
            // last's bottom and span's top should fit inside the leading
            // tolerance. ~keep
            let last_bottom = last.bbox.top();
            // ~keep
            let span_top = span.bbox.top();
            let vertical_gap = (last_bottom - span_top).abs();

            let indent_ok = span.bbox.left() >= last.bbox.left() - indent_tolerance
                && span.bbox.left() <= last.bbox.left() + indent_tolerance;
            let leading_ok = vertical_gap <= leading_tolerance;

            if size_ok && bold_ok && indent_ok && leading_ok {
                current.push(idx);
            } else {
                runs.push(std::mem::take(&mut current));
                current.push(idx);
            }
        }
        if !current.is_empty() {
            runs.push(current);
        }

        // A run becomes a HeadingRun only when it spans ≥ 2 distinct
        // lines. Single-line bold spans (inline emphasis, lone short
        // headings) don't need locking — XY-cut handles them correctly
        // already, and locking them would be a no-op for the splitter
        // but adds overhead. ~keep
        runs.into_iter()
            .filter_map(|span_indices| {
                if span_indices.len() < 2 {
                    return None;
                }
                let mut distinct_lines = std::collections::BTreeSet::new();
                for &i in &span_indices {
                    distinct_lines.insert(spans[i].bbox.top().round() as i32);
                }
                if distinct_lines.len() < 2 {
                    return None;
                }
                Some(HeadingRun {
                    combined_bbox: union_bboxes(spans, &span_indices),
                    span_indices,
                })
            })
            .collect()
    }

    /// Whether `span`, which shares a line with the current run's last span
    /// `last`, in fact belongs to a DIFFERENT column — in which case it is
    /// neither run material nor a reason to close the run.
    ///
    /// The answer is geometric and exact: the two spans are in different
    /// columns when one ends before the gutter and the other begins after it.
    /// A span that straddles the gutter (a full-width banner heading) is in
    /// neither column and is never separated from anything by this test.
    ///
    /// `None` — a caller with no gutter to give — answers `false`, so the
    /// fold is unconditional exactly as it was before GH#1757. A width-based
    /// stand-in was measured and rejected: on a page with no detected gutter
    /// the gaps it would have to reject are the same size as the gaps inside
    /// legitimate heading lines (median 82 pt, half at or above the 79 pt of
    /// GH#1757's own gutter, on one corpus document), so no threshold
    /// separates the two populations and every such page would change. ~keep
    fn same_line_span_belongs_to_other_column(span: &TextSpan, last: &TextSpan, column_gutter: Option<f32>) -> bool {
        let Some(gutter_x) = column_gutter else {
            return false;
        };
        let (left_box, right_box) = if span.bbox.left() <= last.bbox.left() {
            (span, last)
        } else {
            (last, span)
        };
        left_box.bbox.right() <= gutter_x && right_box.bbox.left() >= gutter_x
    }

    /// Build a synthetic span list where each detected `HeadingRun`
    /// collapses to ONE wide synthetic span carrying the union bbox.
    /// Non-heading spans pass through unchanged.
    ///
    /// Returns:
    /// - `synthetic`: the input to `partition_indexed`.
    /// - `synthetic_origin[k]`: indices of ORIGINAL spans backing
    ///   synthetic span `k`. Length 1 for pass-throughs, ≥ 2 for
    ///   heading-run placeholders. Used by `expand_blocks` to project
    ///   partition output back into original-span space.
    fn synthesize_for_partition(&self, spans: &[TextSpan], runs: &[HeadingRun]) -> (Vec<TextSpan>, Vec<Vec<usize>>) {
        let mut in_run: Vec<Option<usize>> = vec![None; spans.len()];
        for (r_idx, run) in runs.iter().enumerate() {
            for &i in &run.span_indices {
                in_run[i] = Some(r_idx);
            }
        }

        let mut synthetic: Vec<TextSpan> = Vec::with_capacity(spans.len());
        let mut origins: Vec<Vec<usize>> = Vec::with_capacity(spans.len());
        let mut emitted_run = vec![false; runs.len()];

        for (i, span) in spans.iter().enumerate() {
            match in_run[i] {
                None => {
                    synthetic.push(span.clone());
                    origins.push(vec![i]);
                }
                Some(r_idx) if !emitted_run[r_idx] => {
                    let run = &runs[r_idx];
                    let mut placeholder = span.clone();
                    placeholder.bbox = run.combined_bbox;
                    // Concatenate the run's text with single spaces so
                    // is_single_column_region's core-width estimate is
                    // proportional to the actual heading length, not the
                    // single first-line fragment. ~keep
                    let mut combined_text = String::new();
                    for (k, &si) in run.span_indices.iter().enumerate() {
                        if k > 0 {
                            combined_text.push(' ');
                        }
                        combined_text.push_str(&spans[si].text);
                    }
                    placeholder.text = combined_text;
                    synthetic.push(placeholder);
                    origins.push(run.span_indices.clone());
                    emitted_run[r_idx] = true;
                }
                Some(_) => {}
            }
        }

        (synthetic, origins)
    }

    /// Project partition groups from synthetic-span space back into
    /// original-span space, expanding each heading-run placeholder into
    /// its constituent original spans (in their original ordering).
    fn expand_blocks(
        &self,
        synth_groups: Vec<Vec<usize>>,
        original: &[TextSpan],
        synthetic_origin: &[Vec<usize>],
    ) -> Vec<Vec<TextSpan>> {
        synth_groups
            .into_iter()
            .map(|group| {
                let mut out = Vec::with_capacity(group.len());
                for synth_idx in group {
                    for &orig_idx in &synthetic_origin[synth_idx] {
                        out.push(original[orig_idx].clone());
                    }
                }
                out
            })
            .collect()
    }

    /// Index-based recursive partitioning — returns groups of indices into the input span slice.
    ///
    /// Avoids cloning TextSpan at every recursive split level. Spans are only
    /// read through shared reference; indices are partitioned instead.
    fn partition_indexed(&self, all_spans: &[TextSpan], indices: &[usize]) -> Vec<Vec<usize>> {
        self.partition_indexed_depth(all_spans, indices, 0)
    }

    /// Depth-bounded recursive partition. `find_vertical_split_indexed` permits
    /// singleton peels, so without a cap a page with many distinct-Y
    /// header/footer strips can recurse O(n) deep (O(n² log n) work);
    /// `MAX_PARTITION_DEPTH` bounds it.
    fn partition_indexed_depth(&self, all_spans: &[TextSpan], indices: &[usize], depth: u32) -> Vec<Vec<usize>> {
        if indices.is_empty() {
            return Vec::new();
        }

        // Base case: small region, don't split further via the recursive
        // partitioner. Below `min_spans_for_split`, the statistical
        // prose/table classifiers (`classify_region_kind`,
        // `detect_two_column_prose`, `detect_narrow_gutter_prose`) all have
        // their own internal minimum-span floors (6/8/24) far above this
        // one and unconditionally decline to classify — so a flat
        // "impose Y-then-X row-major order" was applied even to a genuine
        // sparse 2-column page (a 2-column, 2-row prose page —
        // 4 spans — read back row-major/interleaved instead of
        // column-major).
        //
        // A geometric gutter check alone can't distinguish "sparse 2-column
        // prose" from "a 2x2 row-major table" at this scale either — both
        // produce an identical clean-gutter signature, and the table-row
        // guard inside `find_horizontal_split_indexed` needs >=3 rows to
        // reach confidence, so it's a no-op at 2 rows. Rather than commit
        // to a (left, right) column grouping we can't justify being
        // correct, fall back to the page's own content-stream emission
        // order when a clean gutter exists at all (PDFium parity, per the
        // reporter's own cross-tool probe: PDFium performs no prose/table
        // decision here either, it just follows stream order). This
        // relies on the empirical tendency of table generators to emit
        // cells row-major and column-generators to emit column-major, in
        // the absence of any other geometric signal being decidable at
        // this scale. Falls back to the flat Y-then-X sort when no clean
        // gutter is found at all (ordinary short single-column snippets). ~keep
        if indices.len() < self.min_spans_for_split {
            if self.find_horizontal_split_indexed(all_spans, indices).is_some() {
                let mut stream_order: Vec<usize> = indices.to_vec();
                stream_order.sort_by_key(|&i| all_spans[i].sequence);
                return vec![stream_order];
            }
            return vec![self.sort_indices(all_spans, indices)];
        }

        if depth >= MAX_PARTITION_DEPTH {
            return vec![self.sort_indices(all_spans, indices)];
        }

        // Two-column-prose probe BEFORE the
        // single-column short-circuit. Tight gutters (~10-15pt) that
        // sit below `min_valley_width` defeat the standard projection-
        // valley detector, and the wide+dense heuristic inside
        // `is_single_column_region` mis-classifies the body as one
        // column because each line's bbox spans the narrow gutter.
        // The probe positively identifies the 2-column-prose shape
        // (gutter-radius left-edge clusters + ≥6 narrow lines +
        // classify_region_kind == Prose) and only fires when ALL of
        // those signals agree. Critically, the Prose gate prevents
        // the false positive that reverted earlier attempts on a
        // 2-column sub-region of the google_doc population table
        // (mean_chars < 8 → Table → bail).
        //
        // **Band-separation first**: when the probe would fire AND a
        // clean vertical band-separation (top header / body / bottom
        // footer) is available, peel the band off BEFORE the column
        // cut. Without this step, full-width header / footer rows
        // get absorbed into one of the two column halves and end up
        // mid-page in reading order — the failure mode on the
        // 1256-page French Bible where the chapter-header
        // band and page-number footer were full-width and span the
        // gutter. The signal for "band": a vertical split whose
        // smaller side has ≤ 25 % of the region's spans (a tight
        // band relative to the body it sits next to).
        // Two-column-prose detector based on line-start clustering.
        // When it fires, peel any wide Y-band first (title / authors
        // / abstract / footer often span the gutter) before the
        // column cut, so they don't get fragmented across columns.
        // Each peeled band is re-classified inside the recursive
        // call.
        //
        // Classify once and pass to both prose detectors below; each gated on
        // `classify_region_kind == Prose` and re-ran the same line clustering. ~keep
        let region_kind = self.classify_region_kind(all_spans, indices);
        if let Some(gutter_x) = self.detect_two_column_prose(all_spans, indices, region_kind) {
            if let Some((above, below)) = self.find_vertical_split_indexed(all_spans, indices) {
                tracing::trace!(
                    above = above.len(),
                    below = below.len(),
                    "peeling Y-band before column cut"
                );
                let mut result = self.partition_indexed_depth(all_spans, &above, depth + 1);
                result.extend(self.partition_indexed_depth(all_spans, &below, depth + 1));
                return result;
            }
            let (left, right): (Vec<usize>, Vec<usize>) = indices
                .iter()
                .copied()
                .partition(|&i| all_spans[i].bbox.left() < gutter_x);
            if !left.is_empty() && !right.is_empty() {
                tracing::trace!(
                    gutter_x,
                    left = left.len(),
                    right = right.len(),
                    "two-column-prose detected"
                );
                let mut result = self.partition_indexed_depth(all_spans, &left, depth + 1);
                result.extend(self.partition_indexed_depth(all_spans, &right, depth + 1));
                return result;
            }
        }

        // Narrow-gutter prose detector — second pass for layouts
        // where the line-start cluster shape is masked by outlier
        // singletons (title / caption / equation rows scattering
        // extra clusters that block the primary detector). Cuts
        // directly at the gap-cluster centre WITHOUT peeling a
        // Y-band first: for these pages `find_vertical_split`
        // tends to fire on mid-body paragraph gaps and bisect
        // the body across the peel — both halves then lose
        // enough gutter signal that the column cut never reaches
        // them on recursion. ~keep
        if let Some(gutter_x) = self.detect_narrow_gutter_prose(all_spans, indices, region_kind) {
            let (left, right): (Vec<usize>, Vec<usize>) = indices
                .iter()
                .copied()
                .partition(|&i| all_spans[i].bbox.left() < gutter_x);
            if !left.is_empty() && !right.is_empty() {
                tracing::trace!(
                    gutter_x,
                    left = left.len(),
                    right = right.len(),
                    "narrow-gutter prose detected"
                );
                let mut result = self.partition_indexed_depth(all_spans, &left, depth + 1);
                result.extend(self.partition_indexed_depth(all_spans, &right, depth + 1));
                return result;
            }
        }

        // Detect single-column body text up-front and skip all spatial
        // splits. Real body text has density dips (indented code, short
        // last-lines, paragraph breaks) that would otherwise trigger
        // spurious horizontal (column) or vertical (row) splits,
        // scrambling reading order. The subsequent sort-by-Y already
        // handles row order within a column. ~keep
        if self.is_single_column_region(all_spans, indices) {
            return vec![self.sort_indices(all_spans, indices)];
        }

        // GH#1808: a figure legend, a running header, or a table's own caption runs the
        // FULL width of the region. `find_horizontal_split_indexed` below assigns content
        // by inked width majority per LINE (GH#1808 fix), which already keeps a torn line
        // whole, but a full-width line still ends up entirely on one side of a column cut
        // that should not have run through it at all -- reading the legend as a body line
        // of whichever column it landed on, instead of as its own band before or after
        // both columns. Peeling such a run off as its own band first, rather than refusing
        // the column cut outright, is what keeps ordinary two-column prose (a full-width
        // title, an abstract header, a running header, a footer) from losing its column
        // split entirely: `partition_region` already does the same for Y-bands via
        // `find_vertical_split_indexed` peeling ahead of `detect_two_column_prose`, for the
        // same reason. ~keep
        if let Some(bands) = self.peel_full_width_line_bands(all_spans, indices) {
            let mut result = Vec::new();
            for band in bands {
                result.extend(self.partition_indexed_depth(all_spans, &band, depth + 1));
            }
            return result;
        }

        // A gutter is empty. A single-column prose band above a ruled table (an
        // installation manual's settings page: steps 1-5, then a parameter table whose
        // columns open a valley at x 246) is not two columns, but the projection behind the
        // column cut skips every span wider than 55 % of the region -- most of the prose --
        // so the table's own valley is all it sees. Cut there, `partition_lines_at` sends
        // each prose line to its ink majority: the two longest steps went right, into the
        // table's column, and were read after the next heading. When the cut runs through
        // such a band, peel the band off with whatever lies above and below it instead, and
        // let each part find its own cut. ~keep
        if let Some((split_x, _, _)) = self.find_horizontal_split_with_x(all_spans, indices)
            && let Some(parts) = prose_band_split(all_spans, indices, split_x)
            && parts.len() >= 2
        {
            let mut result = Vec::new();
            for part in parts {
                result.extend(self.partition_indexed_depth(all_spans, &part, depth + 1));
            }
            return result;
        }

        let split_h = |s: &Self, sp: &[TextSpan], idx: &[usize]| s.find_horizontal_split_indexed(sp, idx);
        let split_v = |s: &Self, sp: &[TextSpan], idx: &[usize]| s.find_vertical_split_indexed(sp, idx);

        let first_split = if self.prefer_horizontal { split_h } else { split_v };
        let second_split = if self.prefer_horizontal { split_v } else { split_h };

        if let Some((a, b)) = first_split(self, all_spans, indices) {
            let mut result = self.partition_indexed_depth(all_spans, &a, depth + 1);
            result.extend(self.partition_indexed_depth(all_spans, &b, depth + 1));
            return result;
        }

        if let Some((a, b)) = second_split(self, all_spans, indices) {
            let mut result = self.partition_indexed_depth(all_spans, &a, depth + 1);
            result.extend(self.partition_indexed_depth(all_spans, &b, depth + 1));
            return result;
        }

        vec![self.sort_indices(all_spans, indices)]
    }

    /// Classifier verdict for a region — used to gate the tight-gutter
    /// column-split path so the same XY-cut recursion no longer
    /// corrupts table cells (the lesson).
    ///
    /// See the inline post-mortem at lines 73–101: two prior attempts at
    /// the multi-column-prose fix were reverted by the 70-PDF sweep when
    /// they accidentally fired on a 2-column sub-region of a real table
    /// and reordered digits. The fix has to *positively identify prose*
    /// before allowing the tight cut — not merely *fail to identify
    /// table*. This classifier is that positive identification.
    fn classify_region_kind(&self, all_spans: &[TextSpan], indices: &[usize]) -> RegionKind {
        if indices.len() < 6 {
            return RegionKind::Mixed;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width <= 10.0 {
            return RegionKind::Mixed;
        }

        let mut lines: std::collections::BTreeMap<i32, (f32, f32, usize)> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            let nonws_chars = s.text.chars().filter(|c| !c.is_whitespace()).count();
            let entry = lines.entry(y_key).or_insert((f32::MAX, f32::MIN, 0));
            entry.0 = entry.0.min(s.bbox.left());
            entry.1 = entry.1.max(s.bbox.right());
            entry.2 += nonws_chars;
        }

        let line_count = lines.len();
        if line_count < 6 {
            // Too few lines to be a substantial prose body. Headings,
            // captions, single paragraphs all land here — leave them to
            // the default XY-cut behaviour. ~keep
            return RegionKind::Mixed;
        }

        // Per-line statistics: average char count and the count of
        // "narrow" lines whose extent < 0.6 × region_width (a column-half
        // line) and "wide" lines whose extent ≥ 0.6 × region_width (a
        // body-text or table-row line). Table cells are narrow; tables
        // have many such narrow lines but with very short content. ~keep
        let mut total_chars = 0usize;
        let mut narrow_lines = 0usize;
        let mut wide_lines = 0usize;
        for (left, right, chars) in lines.values() {
            total_chars += chars;
            let extent = (*right - *left).max(0.0);
            if extent < region_width * 0.6 {
                narrow_lines += 1;
            } else {
                wide_lines += 1;
            }
        }
        let mean_chars = total_chars as f32 / line_count as f32;

        // PROSE: tall stack of wide lines OR tall stack of half-column
        // lines with substantial content per line.
        //   - mean_chars > 20: real prose, not table cells
        //   - line_count ≥ 6: substantial column
        //   - either:
        //     * majority of lines are wide (single-column body), OR
        //     * majority of lines are narrow with mean_chars > 20
        //       (two half-column lines with prose content) ~keep
        let mostly_wide = wide_lines * 2 > line_count;
        let mostly_narrow = narrow_lines * 2 > line_count;
        if mean_chars > 20.0 && (mostly_wide || mostly_narrow) {
            return RegionKind::Prose;
        }

        // SHORT-LINE PROSE (short-verse two-column bodies): the
        // `mean_chars > 20` guard above deliberately rejected short-verse
        // two-column bodies (Bible / lexicon editions — a verse fragment
        // per column-line is often < 20 non-whitespace chars) along with
        // short-cell tables. The guard was doing two jobs at once. Here we
        // re-admit ONLY the short-line case that carries a *strong central
        // gutter corridor* a short-cell table cannot fake: a single
        // persistent vertical gutter near the region centre, present on a
        // high fraction of lines, with balanced left/right char mass and
        // ≤ 2 left-edge clusters. A label+data table fails this on
        // concentration/coverage (its gaps scatter across cell
        // boundaries), centre (the dominant gap sits off-centre),
        // char-balance (the label column is tiny), or left-edge clusters
        // (≥ 3 columns). The long-line accept path above is byte-unchanged. ~keep
        if mean_chars <= 20.0 && self.short_line_central_corridor_prose(all_spans, indices, x_min, region_width) {
            return RegionKind::Prose;
        }

        // TABLE: lots of narrow lines, short content per line (mean_chars
        // < 8). The google_doc population table —
        // the canonical regression that reverted attempts 1 & 2 — sits
        // squarely here (digit-only cells, ≤ 7 chars each). ~keep
        if mean_chars < 8.0 {
            return RegionKind::Table;
        }

        // Anything in between (e.g. captions with headings, mixed
        // figure-and-text bands) → don't risk the tight cut. ~keep
        RegionKind::Mixed
    }

    /// Short-line two-column-prose admission.
    ///
    /// Called from `classify_region_kind` ONLY for the short-line case
    /// (`mean_chars <= 20`) that the long-line prose guard rejects. A
    /// short-verse two-column body (verse-per-line bibles/lexicons) has
    /// short lines yet a strong, table-independent central gutter; a
    /// short-cell numeric table has short lines and NO such corridor.
    ///
    /// Returns `true` only when ALL of the following hold — each one a
    /// length-independent discriminator a short-cell label+data table
    /// cannot satisfy:
    ///   - a single persistent vertical gutter exists: per-line largest
    ///     within-line gap clusters at one X (10 pt radius) covering
    ///     **≥ 70 %** of gap-bearing lines (concentration) and present on
    ///     **≥ 60 %** of all lines (coverage) — a table's dominant gap
    ///     scatters across cell boundaries and appears on a minority of
    ///     rows;
    ///   - that gutter sits near the region centre: offset ∈
    ///     **[0.30, 0.70]·region_width** — a label+data table's dominant
    ///     gap sits off-centre;
    ///   - **left/right char balance:** non-whitespace char mass on each
    ///     side of the gutter is **≥ 35 %** of the total — a label column
    ///     is lopsided (one side is tiny numeric labels);
    ///   - **≤ 2 left-edge clusters** left of the gutter (30 pt radius) —
    ///     a real two-column body starts each column at one X; an
    ///     N-column table has ≥ 3 left-edge clusters (the fix-534
    ///     `left_edge_clusters >= 3 → Mixed` rule).
    fn short_line_central_corridor_prose(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        x_min: f32,
        region_width: f32,
    ) -> bool {
        if region_width <= 0.0 {
            return false;
        }

        // Re-cluster spans into lines, keeping PER-SPAN (left, right, chars)
        // so we can find the within-line gutter gap and split char mass. ~keep
        let mut lines: std::collections::BTreeMap<i32, Vec<(f32, f32, usize)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            let nonws = s.text.chars().filter(|c| !c.is_whitespace()).count();
            lines
                .entry(y_key)
                .or_default()
                .push((s.bbox.left(), s.bbox.right(), nonws));
        }
        let total_lines = lines.len();
        if total_lines == 0 {
            return false;
        }

        // Per-line: largest within-line gap and its midpoint X. A gap of
        // ≥ 6 pt suppresses ordinary 2–5 pt word spacing. ~keep
        const MIN_GAP_PT: f32 = 6.0;
        let mut gap_positions: Vec<f32> = Vec::new();
        for line_spans in lines.values() {
            if line_spans.len() < 2 {
                continue;
            }
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let mut largest_gap = 0.0_f32;
            let mut largest_mid = 0.0_f32;
            for w in sorted.windows(2) {
                let gap = w[1].0 - w[0].1;
                if gap > largest_gap {
                    largest_gap = gap;
                    largest_mid = (w[0].1 + w[1].0) * 0.5;
                }
            }
            if largest_gap >= MIN_GAP_PT {
                gap_positions.push(largest_mid);
            }
        }
        if gap_positions.is_empty() {
            return false;
        }

        const CLUSTER_RADIUS_PT: f32 = 10.0;
        let mut sorted_gaps = gap_positions.clone();
        sorted_gaps.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
        let mut best_size = 0usize;
        let mut best_center = 0.0_f32;
        for &pivot in &sorted_gaps {
            let lo = pivot - CLUSTER_RADIUS_PT;
            let hi = pivot + CLUSTER_RADIUS_PT;
            let mut count = 0usize;
            let mut sum = 0.0_f32;
            for &g in &sorted_gaps {
                if g >= lo && g <= hi {
                    count += 1;
                    sum += g;
                }
            }
            if count > best_size {
                best_size = count;
                best_center = sum / count as f32;
            }
        }
        if best_size == 0 {
            return false;
        }

        // Concentration ≥ 70 % of gap-bearing lines at one X. ~keep
        if best_size * 10 < gap_positions.len() * 7 {
            return false;
        }
        // Coverage ≥ 60 % of ALL lines carry the corridor. ~keep
        if best_size * 10 < total_lines * 6 {
            return false;
        }
        let gutter_offset = best_center - x_min;
        if gutter_offset < region_width * 0.30 || gutter_offset > region_width * 0.70 {
            return false;
        }

        // Left/right non-whitespace char balance about the corridor:
        // each side ≥ 35 % of total. A label-column table is lopsided. ~keep
        let mut left_chars = 0usize;
        let mut right_chars = 0usize;
        for line_spans in lines.values() {
            for &(l, r, chars) in line_spans {
                let mid = (l + r) * 0.5;
                if mid < best_center {
                    left_chars += chars;
                } else {
                    right_chars += chars;
                }
            }
        }
        let total_chars = left_chars + right_chars;
        if total_chars == 0 {
            return false;
        }
        if (left_chars as f32) < total_chars as f32 * 0.35 || (right_chars as f32) < total_chars as f32 * 0.35 {
            return false;
        }

        // ≤ 2 left-edge clusters left of the corridor (30 pt radius). A
        // real two-column body starts its left column at one X (one
        // cluster, maybe two counting a paragraph indent); an N-column
        // table left of the corridor has several cell-start X's → ≥ 3
        // clusters. Cluster EVERY span left-edge that lies left of the
        // corridor (not just each line's minimum) so multi-column cell
        // starts are not collapsed into one cluster. ~keep
        const LEFT_CLUSTER_RADIUS_PT: f32 = 30.0;
        let mut clusters: Vec<(f32, usize)> = Vec::new();
        for line_spans in lines.values() {
            for &(l, _, _) in line_spans {
                if l >= best_center {
                    continue;
                }
                if let Some(c) = clusters
                    .iter_mut()
                    .find(|(c, _)| (*c - l).abs() <= LEFT_CLUSTER_RADIUS_PT)
                {
                    let count = c.1 as f32;
                    c.0 = (c.0 * count + l) / (count + 1.0);
                    c.1 += 1;
                } else {
                    clusters.push((l, 1));
                }
            }
        }
        // Drop singleton/noise clusters (< 2 lines) before counting, so a
        // lone outlier left-edge doesn't inflate the count. ~keep
        let dominant_left_clusters = clusters.iter().filter(|(_, n)| *n >= 2).count();
        if dominant_left_clusters >= 3 {
            return false;
        }

        true
    }

    /// Two-column-prose probe — does this region look like two
    /// side-by-side columns of prose with a tight gutter (~10-15pt)?
    ///
    /// Called from `is_single_column_region` when the wide+dense
    /// heuristic would otherwise short-circuit the region as
    /// single-column. Distinguishing signal: most lines fit inside
    /// **one** half of the region width (column-half lines), and the
    /// left edges cluster into exactly **two** groups separated by
    /// approximately half the region width.
    ///
    /// Gated on `classify_region_kind == Prose` so the same machinery
    /// doesn't fire on a 2-column sub-region of a table (an earlier
    /// failure mode).
    ///
    /// Returns `Some(gutter_x)` when a 2-column prose layout is
    /// detected — the caller treats that as a non-single-column verdict
    /// and lets `find_horizontal_split_indexed` cut at the gutter.
    fn detect_two_column_prose(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        region_kind: RegionKind,
    ) -> Option<f32> {
        if indices.len() < 8 {
            return None;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width < 200.0 {
            // Real two-column bodies span at least ~200pt (the
            // narrowest two-column layout in the corpus is ~250pt for a
            // letter-page body inside ~250pt margins). ~keep
            return None;
        }

        // Cluster spans into lines by rounded Y. Keep PER-SPAN
        // (left, right) data so we can detect within-line gaps —
        // the canonical multi-column interleave puts
        // a left-col span (left=82) and a right-col span (left=312)
        // on the same Y baseline. The whole-line bbox.right -
        // bbox.left = 358 pt looks "wide" (358 > 0.6 × 500 = 300)
        // even though each side is a narrow column half. ~keep
        let mut lines_spans: std::collections::BTreeMap<i32, Vec<(f32, f32)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            lines_spans
                .entry(y_key)
                .or_default()
                .push((s.bbox.left(), s.bbox.right()));
        }
        if lines_spans.len() < 6 {
            return None;
        }

        // For each line, find the largest gap between adjacent spans.
        // A line is treated as multiple "half-lines" if a gap ≥ 10 pt
        // splits it; each side of the gap contributes its leftmost-x
        // to `narrow_lefts`. This is the lesson: the row-by-
        // row interleave shape spans the gutter as bbox
        // but has a clear gap within each line. ~keep
        let narrow_threshold = region_width * 0.6;
        let intra_line_gap_threshold = 10.0_f32;
        let mut narrow_lefts: Vec<f32> = Vec::new();
        // Count "narrow" lines for the majority check — a line with
        // a within-line gap contributes 1 to this count regardless of
        // how many half-lines it produces, so the majority threshold
        // stays comparable to single-column reasoning. ~keep
        let mut narrow_line_count = 0usize;
        for line_spans in lines_spans.values() {
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let mut largest_gap = 0.0_f32;
            let mut split_idx: Option<usize> = None;
            for (i, w) in sorted.windows(2).enumerate() {
                let gap = w[1].0 - w[0].1;
                if gap > largest_gap {
                    largest_gap = gap;
                    split_idx = Some(i);
                }
            }
            let line_left = sorted.first().map(|(l, _)| *l).unwrap_or(0.0);
            let line_right = sorted.last().map(|(_, r)| *r).unwrap_or(0.0);
            let line_extent = (line_right - line_left).max(0.0);

            if let Some(si) = split_idx
                && largest_gap >= intra_line_gap_threshold
            {
                narrow_lefts.push(line_left);
                if let Some(&(right_side_left, _)) = sorted.get(si + 1) {
                    narrow_lefts.push(right_side_left);
                }
                narrow_line_count += 1;
                continue;
            }

            if line_extent < narrow_threshold {
                narrow_lefts.push(line_left);
                narrow_line_count += 1;
            }
        }
        // Majority of lines must be narrow — otherwise this isn't a
        // 2-column body, it's a single-column body with a few short
        // last-lines. ~keep
        if narrow_line_count * 2 < lines_spans.len() {
            return None;
        }

        // Cluster the narrow left-edges. Two clusters separated by
        // approximately half the region width = 2-column prose. ~keep
        let cluster_radius = 30.0_f32;
        let mut clusters: Vec<(f32, usize)> = Vec::new();
        for &x in &narrow_lefts {
            if let Some(c) = clusters.iter_mut().find(|(c, _)| (*c - x).abs() <= cluster_radius) {
                let count = c.1 as f32;
                c.0 = (c.0 * count + x) / (count + 1.0);
                c.1 += 1;
            } else {
                clusters.push((x, 1));
            }
        }

        // Want exactly 2 substantial clusters separated by ~half-width.
        // ≥ 3 clusters = either a table or a band-mixed region — bail. ~keep
        if clusters.len() != 2 {
            return None;
        }
        clusters.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
        let (c1_x, c1_n) = clusters[0];
        let (c2_x, c2_n) = clusters[1];

        // Each cluster needs substantial coverage — ≥ 3 lines, or 20 %
        // of the line count, whichever is larger. Reject lopsided
        // shapes (header + body-paragraph). ~keep
        let min_cluster = 3usize.max(narrow_lefts.len() / 5);
        if c1_n < min_cluster || c2_n < min_cluster {
            return None;
        }

        // Gap between cluster centres ≥ 30 % of region width (the
        // gutter + right-column left-margin). For a tight gutter of
        // ~12pt with two ~250pt columns the gap is ~250pt out of 512pt
        // → ~49 %, well above the floor. ~keep
        let gap = c2_x - c1_x;
        if gap < region_width * 0.30 {
            return None;
        }

        // Positive identification of prose — required by the
        // classifier to avoid the google_doc 2-col table
        // sub-region false positive. ~keep
        if region_kind != RegionKind::Prose {
            return None;
        }

        // Gutter midpoint as the cut. The cluster centres are the left
        // edges of the two columns; the gutter sits between the right
        // edge of column 1 and the left edge of column 2. We don't
        // track right edges per cluster, so approximate the gutter
        // centre as halfway between the two cluster centres — that's
        // close enough; the actual partition uses `bbox.left()` per
        // span so individual spans land cleanly on either side. ~keep
        let gutter_x = (c1_x + c2_x) * 0.5;
        Some(gutter_x)
    }

    /// Second-pass 2-column-prose detector for the narrow-gutter case
    /// that `detect_two_column_prose` (the line-start-cluster detector)
    /// misses.
    ///
    /// Two-column papers that emit body text at character-cluster
    /// granularity (each glyph its own span) confuse the line-start
    /// detector: titles, captions, and equation labels contribute
    /// outlier singleton clusters in addition to the two body
    /// columns, so the `clusters.len() != 2` gate rejects. Their
    /// gutters are also often narrower than `min_valley_width` so
    /// the primary projection-valley path in
    /// `find_horizontal_split_indexed` rejects as well.
    ///
    /// Distinguishing signal that works regardless of outlier rows:
    /// the **largest within-line gap** on each body line lives at
    /// roughly the same X coordinate (the gutter) across a strong
    /// majority of lines. Cluster those gap positions; if one cluster
    /// covers ≥ 60 % of the body lines AND the region classifies as
    /// `Prose`, the page is two-column prose and the cluster centre
    /// is the gutter X.
    ///
    /// Returns the gutter X coordinate (an actual gap position, not
    /// a midpoint estimate) when the pattern is detected.
    ///
    /// The Prose-classifier gate keeps tables out: table rows have
    /// their largest gap at variable X across rows (different cell
    /// widths), so the gap-position cluster never dominates.
    fn detect_narrow_gutter_prose(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        region_kind: RegionKind,
    ) -> Option<f32> {
        if indices.len() < 24 {
            return None;
        }
        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width < 200.0 {
            return None;
        }

        let mut lines: std::collections::BTreeMap<i32, Vec<(f32, f32)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            lines.entry(y_key).or_default().push((s.bbox.left(), s.bbox.right()));
        }
        if lines.len() < 12 {
            return None;
        }

        // For each line, find the largest within-line gap (≥ 6 pt
        // suppresses ordinary word-spacing of 2–5 pt). Record the gap's
        // midpoint X. ~keep
        const MIN_GAP_PT: f32 = 6.0;
        let mut gap_positions: Vec<f32> = Vec::new();
        for line_spans in lines.values() {
            if line_spans.len() < 2 {
                continue;
            }
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let mut largest_gap = 0.0_f32;
            let mut largest_mid = 0.0_f32;
            for w in sorted.windows(2) {
                let gap = w[1].0 - w[0].1;
                if gap > largest_gap {
                    largest_gap = gap;
                    largest_mid = (w[0].1 + w[1].0) * 0.5;
                }
            }
            if largest_gap >= MIN_GAP_PT {
                gap_positions.push(largest_mid);
            }
        }

        // Need at least 12 gap-bearing lines to cluster — fewer is
        // statistical noise. ~keep
        if gap_positions.len() < 12 {
            return None;
        }

        // Cluster the gap positions with a 10 pt radius (tight; the
        // gutter is at one specific X with minor line-to-line drift).
        // Sliding-window two-pointer scan over the sorted positions —
        // both `left` and `right` only advance forward, so total
        // work is O(n) instead of the previous O(n²) pivot scan
        // (thesis-style PDFs with hundreds of gap-bearing rows pay
        // visibly in that nested loop). ~keep
        const CLUSTER_RADIUS_PT: f32 = 10.0;
        let mut sorted_gaps = gap_positions.clone();
        sorted_gaps.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
        // Prefix sums let us read window-sum in O(1) given (left, right). ~keep
        let mut prefix: Vec<f32> = Vec::with_capacity(sorted_gaps.len() + 1);
        prefix.push(0.0);
        for &x in &sorted_gaps {
            prefix.push(prefix.last().unwrap() + x);
        }
        let mut best_size = 0usize;
        let mut best_center = 0.0_f32;
        let mut left = 0usize;
        let mut right = 0usize;
        for &pivot in &sorted_gaps {
            while left < sorted_gaps.len() && sorted_gaps[left] < pivot - CLUSTER_RADIUS_PT {
                left += 1;
            }
            while right < sorted_gaps.len() && sorted_gaps[right] <= pivot + CLUSTER_RADIUS_PT {
                right += 1;
            }
            let count = right - left;
            let sum = prefix[right] - prefix[left];
            if count > best_size {
                best_size = count;
                best_center = sum / count as f32;
            }
        }

        // Concentration: ≥ 70 % of gap-bearing lines cluster at the
        // same X. Distinguishes 2-col prose (one gutter) from
        // tables (gaps at several cell boundaries, lower
        // concentration). ~keep
        if best_size * 10 < gap_positions.len() * 7 {
            return None;
        }
        if best_size < 12 {
            return None;
        }
        if best_size * 5 < lines.len() {
            return None;
        }

        let gutter_offset = best_center - x_min;
        if gutter_offset < region_width * 0.2 || gutter_offset > region_width * 0.8 {
            return None;
        }

        // Prose gate — same safety as `detect_two_column_prose`.
        // Tables with narrow cell gaps fail the classifier
        // (`mean_chars < 8` → `Table`), preventing the gap-cluster
        // signal from misfiring on tabular content. Short-verse
        // two-column bodies now also pass this gate: although
        // their `mean_chars <= 20`, `classify_region_kind`'s short-line
        // central-corridor admission arm returns `Prose` for them, so a
        // routed short-verse body is cut here rather than re-collapsed.
        // ~keep
        if region_kind != RegionKind::Prose {
            return None;
        }

        Some(best_center)
    }

    /// Heuristic: does the region look like a single column of body text?
    ///
    /// Called **before** horizontal split attempts. When true, the region
    /// is returned as a single sorted group, bypassing both horizontal
    /// (column) and vertical (row) splits. This prevents XY-Cut from
    /// fragmenting body text at density dips caused by indentation or
    /// short last-lines.
    ///
    /// Detection: cluster spans into lines by rounded top-Y, then count
    /// lines that are both **wide** (extent ≥ 60% region width) and
    /// **dense** (covered ratio ≥ 80%). Body-text lines satisfy both.
    /// Aligned multi-column rows look "wide" because their extent spans
    /// the gutter, but fail the density check because the gutter is empty.
    /// Split `indices` into y-ordered bands at the boundaries of every contiguous run of
    /// FULL-WIDTH lines, or `None` when there is no such run (the overwhelmingly common
    /// case -- the hot path pays one line grouping).
    ///
    /// A line (already a continuous run of ink -- see `group_indices_into_lines`) is
    /// full-width when its inked extent reaches `FULL_WIDTH_LINE_FRACTION` of the region's
    /// own width. GH#1808. ~keep
    fn peel_full_width_line_bands(&self, all_spans: &[TextSpan], indices: &[usize]) -> Option<Vec<Vec<usize>>> {
        let rows = group_indices_into_rows(all_spans, indices);
        if rows.len() < MIN_PEELED_FULL_WIDTH_LINES {
            return None;
        }
        let region_left = indices
            .iter()
            .map(|&i| all_spans[i].bbox.left())
            .fold(f32::MAX, f32::min);
        let region_right = indices
            .iter()
            .map(|&i| all_spans[i].bbox.right())
            .fold(f32::MIN, f32::max);
        let region_width = region_right - region_left;
        if !(region_width.is_finite() && region_width > 0.0) {
            return None;
        }

        let is_full_width = |line: &[usize]| -> bool {
            let line_left = line.iter().map(|&i| all_spans[i].bbox.left()).fold(f32::MAX, f32::min);
            let line_right = line.iter().map(|&i| ink_right(&all_spans[i])).fold(f32::MIN, f32::max);
            (line_right - line_left) / region_width >= FULL_WIDTH_LINE_FRACTION
        };
        // A row is peeled when any of its lines is full width, and then WHOLE (see
        // `group_indices_into_rows`). ~keep
        let flags: Vec<bool> = rows
            .iter()
            .map(|row| row.iter().any(|line| is_full_width(line)))
            .collect();
        let lines: Vec<Vec<usize>> = rows
            .into_iter()
            .map(|row| row.into_iter().flatten().collect())
            .collect();
        if !flags.contains(&true) {
            return None;
        }

        let mut bands: Vec<Vec<usize>> = Vec::new();
        let mut normal: Vec<usize> = Vec::new();
        let mut peeled_any = false;
        let mut index = 0usize;
        while index < lines.len() {
            if !flags[index] {
                normal.extend(&lines[index]);
                index += 1;
                continue;
            }
            let run_start = index;
            while index < lines.len() && flags[index] {
                index += 1;
            }
            let run = &lines[run_start..index];
            if run.len() >= MIN_PEELED_FULL_WIDTH_LINES {
                if !normal.is_empty() {
                    bands.push(std::mem::take(&mut normal));
                }
                bands.push(run.iter().flatten().copied().collect());
                peeled_any = true;
            } else {
                for line in run {
                    normal.extend(line);
                }
            }
        }
        if !normal.is_empty() {
            bands.push(normal);
        }
        (peeled_any && bands.len() >= 2).then_some(bands)
    }

    /// Assign each of `indices`' visual lines, whole, to the side holding the majority of
    /// its inked width -- never split a single line's spans across `split_x`. A line
    /// entirely on one side is unaffected: its majority side is trivially its only side, so
    /// this partitions identically to a per-span `left edge < split_x` test there. Ties (a
    /// line whose inked width splits exactly evenly) go left, matching the pre-existing
    /// left-edge bias. GH#1808. ~keep
    fn partition_lines_at(&self, all_spans: &[TextSpan], indices: &[usize], split_x: f32) -> (Vec<usize>, Vec<usize>) {
        // the side is decided per UNIT. A hanging number and its title are two lines
        // (more than an em apart) but one heading: with a cut through the title (`6.4.3` at
        // x 43-68, its title 85-381, cut at 223.5) the title alone is a genuine crossing
        // line and would move to its majority side without its number. So a line joins the
        // unit before it when that unit is a short LEAD -- a number, a bullet, a label of at
        // most `MAX_LEAD_EM` ems of ink -- and the cut does not run between them. Two column
        // lines on one baseline stay two units even when the cut is elsewhere: a column
        // line is never a short lead. ~keep
        let mut units: Vec<Vec<usize>> = Vec::new();
        for row in group_indices_into_rows(all_spans, indices) {
            let mut unit: Vec<usize> = Vec::new();
            let mut unit_left = f32::MAX;
            let mut unit_right = f32::MIN;
            let mut unit_em = 0.0f32;
            for line in row {
                let line_left = line.iter().map(|&i| all_spans[i].bbox.left()).fold(f32::MAX, f32::min);
                let short_lead = (unit_right - unit_left) <= MAX_LEAD_EM * unit_em;
                let cut_between = unit_right <= split_x && split_x <= line_left;
                if !unit.is_empty() && (cut_between || !short_lead) {
                    units.push(std::mem::take(&mut unit));
                    unit_left = f32::MAX;
                    unit_right = f32::MIN;
                    unit_em = 0.0;
                }
                unit_left = unit_left.min(line_left);
                unit_right = line
                    .iter()
                    .map(|&i| ink_right(&all_spans[i]))
                    .fold(unit_right, f32::max);
                unit_em = line.iter().map(|&i| all_spans[i].font_size).fold(unit_em, f32::max);
                unit.extend(line);
            }
            if !unit.is_empty() {
                units.push(unit);
            }
        }
        let lines = units;
        let mut goes_right: std::collections::HashMap<usize, bool> = std::collections::HashMap::new();
        for line in &lines {
            let mut left_width = 0.0f32;
            let mut right_width = 0.0f32;
            for &i in line {
                let bbox = &all_spans[i].bbox;
                let right = ink_right(&all_spans[i]);
                left_width += (right.min(split_x) - bbox.left()).max(0.0);
                right_width += (right - bbox.left().max(split_x)).max(0.0);
            }
            // only a line that genuinely CROSSES the cut is moved by its majority -- a
            // legend running through the gutter holds ink on both sides (the tear fix's legend lines are
            // near 50/50). A line that merely starts a few points before the cut is a line of
            // the side it starts on: with a cut inside a column, between hanging numbers (x 48)
            // and their titles (from x 79, cut at 91.7), the majority rule sent the title right
            // and left `7.5` behind, splitting the heading. And a line with no ink on either
            // side (a zero-width space a table leaves in its cells) has no majority at all; the
            // tie rule would send it LEFT whatever its position. Both fall back to where the
            // line starts, which is what the per-span partition this replaces did. ~keep
            let ink = left_width + right_width;
            let line_goes_right = if ink <= 0.0 || left_width.min(right_width) < MIN_CROSSING_SHARE * ink {
                line.iter().map(|&i| all_spans[i].bbox.left()).fold(f32::MAX, f32::min) >= split_x
            } else {
                right_width > left_width
            };
            for &i in line {
                goes_right.insert(i, line_goes_right);
            }
        }
        // `Iterator::partition` sends predicate-true items into its FIRST returned vec, so
        // the predicate must test "belongs on the left" to produce `(left, right)` in that
        // order -- inverted, this silently swaps every region's two sides. ~keep
        indices
            .iter()
            .copied()
            .partition(|i| !goes_right.get(i).copied().unwrap_or(false))
    }

    fn is_single_column_region(&self, all_spans: &[TextSpan], indices: &[usize]) -> bool {
        if indices.len() < 3 {
            return false;
        }
        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        for &i in indices {
            x_min = x_min.min(all_spans[i].bbox.left());
            x_max = x_max.max(all_spans[i].bbox.right());
        }
        let region_width = x_max - x_min;
        if region_width <= 10.0 {
            return true;
        }

        // Store both bbox.right and core_right for each span. bbox.right
        // can be over-estimated by extractors (trailing whitespace,
        // stretched advance widths) which makes multi-column lines look
        // like one wide continuous run; core_right (char_count × em) is
        // a conservative fallback used ONLY when adjacent bbox edges
        // overlap (a signal of bbox inflation).
        // ~keep
        let mut lines: std::collections::BTreeMap<i32, Vec<(f32, f32, f32)>> = std::collections::BTreeMap::new();
        for &i in indices {
            let s = &all_spans[i];
            let y_key = s.bbox.top().round() as i32;
            let char_count = s.text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f32;
            let approx_char_width = (s.font_size * 0.45).max(2.5);
            let core_right = s.bbox.left() + char_count * approx_char_width;
            lines
                .entry(y_key)
                .or_default()
                .push((s.bbox.left(), s.bbox.right(), core_right));
        }
        if lines.len() < 3 {
            return false;
        }

        // A real column gutter recurs at roughly the SAME X position
        // across multiple lines. Sparse title-page layouts (Title /
        // Subtitle / Byline) also have wide inter-word gaps, but their
        // gap positions are scattered — not a gutter. Collect all gap
        // positions (mid-gap X), then check whether a consistent cluster
        // of gap positions appears on ≥30% of lines.
        //
        // Gap uses bbox.right, but if adjacent bboxes OVERLAP (classic
        // signature of extractor-inflated bbox widths), re-check with
        // conservative core_right estimates so column detection is not
        // defeated by trailing whitespace inflation. ~keep
        let max_gap = self.min_valley_width;
        let mut gap_positions: Vec<f32> = Vec::new();
        for line_spans in lines.values() {
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            for w in sorted.windows(2) {
                let bbox_gap = w[1].0 - w[0].1;
                let (effective_gap, gap_end_left) = if bbox_gap < 0.0 {
                    (w[1].0 - w[0].2, w[0].2)
                } else {
                    (bbox_gap, w[0].1)
                };
                if effective_gap >= max_gap {
                    gap_positions.push((gap_end_left + w[1].0) * 0.5);
                }
            }
        }
        // Centered-block guard: a CENTERED title/subtitle/
        // byline block (each line horizontally centered, varying widths)
        // produces accidental gap clusters that look like a column
        // gutter — but it is NOT columnar, and treating it as columns
        // scrambles reading order ("Quarterly Inventory Review" centered
        // title read as 3 columns → "Quarterly" / "Spring" / ... ).
        //
        // The distinguishing signal: a REAL multi-column layout has the
        // left column starting at a consistent left edge across rows
        // (low variance of per-line leftmost x). Centered text has its
        // leftmost x scattered (each line centered with a different
        // width). Compute the spread of per-line leftmost edges; if it
        // is large relative to the region width, the block is centered,
        // not columnar, so do NOT treat the gap cluster as a gutter.
        // Centered iff the per-line leftmost edges do NOT share a common
        // left margin. A left-aligned layout (single column OR real
        // multi-column) has most rows starting at the same x (the left
        // margin), so the largest cluster of leftmost edges covers a
        // majority of lines. Centered text has each line's leftmost edge
        // scattered (different per line), so no cluster dominates.
        //
        // Using a cluster fraction (not raw spread) is robust to rows
        // that only contain right-column content — those push the spread
        // up but do not change the fact that the left margin still
        // dominates the remaining rows. (Raw spread mis-classified the
        // two-column test where the last row held only a right cell.) ~keep
        let looks_centered = {
            let mins: Vec<f32> = lines
                .values()
                .map(|ls| ls.iter().map(|(l, _, _)| *l).fold(f32::MAX, f32::min))
                .collect();
            if mins.len() < 2 {
                false
            } else {
                let tol = 10.0_f32;
                // Largest count of leftmost-edges within ±tol of any single edge.
                // Sort once + binary-search the window instead of the O(k^2)
                // all-pairs scan; the max count is a multiset property so this is
                // identical to the pairwise version. ~keep
                let largest = {
                    let mut sorted = mins.clone();
                    sorted.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
                    sorted
                        .iter()
                        .map(|&a| {
                            let lo = sorted.partition_point(|&x| x < a - tol);
                            let hi = sorted.partition_point(|&x| x <= a + tol);
                            hi - lo
                        })
                        .max()
                        .unwrap_or(0)
                };
                (largest as f32) < (mins.len() as f32) * 0.5
            }
        };

        // A SMALL centered block (title / subtitle / byline — few lines,
        // scattered leftmost edges) is treated as a single column so its
        // lines stay in top-to-bottom order and a centered multi-word
        // title is not split into per-word "columns". Gated
        // to <= 6 lines so it only catches title-page-style blocks: a
        // real multi-column body has many lines and is never classified
        // centered here (its left column starts at a consistent margin,
        // giving a small leftmost-spread anyway). ~keep
        if looks_centered && lines.len() <= 6 {
            return true;
        }

        // Cluster gap positions: count, for each observed gap, how many
        // other gaps fall within ±20pt. If any cluster contains gaps
        // from ≥30% of lines, it's a genuine column gutter. ~keep
        if !gap_positions.is_empty() && !looks_centered {
            let cluster_radius = 20.0_f32;
            // Require ≥3 gap positions (or 20% of lines, whichever is
            // larger) clustered within ±20pt. 20% accommodates pages
            // where header/footer/title rows dilute the body-line count
            // but a real multi-column body still dominates. ~keep
            let min_cluster = (3usize).max(lines.len() / 5);
            // Sort once + binary-search each gap's ±radius window instead of the
            // O(k^2) all-pairs scan. Returns false iff some gap's window holds
            // >= min_cluster gaps — identical to the pairwise version. ~keep
            let mut sorted_gaps = gap_positions.clone();
            sorted_gaps.sort_by(|a, b| crate::utils::safe_float_cmp(*a, *b));
            for &pos in &sorted_gaps {
                let lo = sorted_gaps.partition_point(|&p| p < pos - cluster_radius);
                let hi = sorted_gaps.partition_point(|&p| p <= pos + cluster_radius);
                if hi - lo >= min_cluster {
                    return false;
                }
            }
        }

        let width_threshold = region_width * 0.6;
        let mut wide_dense_lines = 0usize;
        for line_spans in lines.values() {
            let mut sorted = line_spans.clone();
            sorted.sort_by(|a, b| crate::utils::safe_float_cmp(a.0, b.0));
            let extent_left = sorted.first().unwrap().0;
            let extent_right = sorted.iter().map(|(_, r, _)| *r).fold(f32::MIN, f32::max);
            let extent = extent_right - extent_left;
            if extent < width_threshold {
                continue;
            }
            // Use core_right (char-count estimate) rather than bbox.right
            // for coverage. bbox.right is inflated by tab characters and
            // trailing whitespace — tab-expanded table rows would otherwise
            // score 100% coverage and be misidentified as dense body text. ~keep
            let mut covered = 0.0f32;
            let mut last_end = f32::MIN;
            for &(l, _, cr) in &sorted {
                let effective_right = cr.min(extent_right);
                let start = l.max(last_end);
                if effective_right > start {
                    covered += effective_right - start;
                    last_end = effective_right;
                }
            }
            if covered >= extent * 0.8 {
                wide_dense_lines += 1;
            }
        }
        wide_dense_lines * 2 >= lines.len()
    }

    /// Find vertical line (X-axis) split using index-based partitioning.
    ///
    /// Rejects lopsided splits where one side contains fewer than ~10% of
    /// the region's spans — those come from single-column pages where
    /// indentation or stray content creates a spurious density dip at
    /// one edge of the projection, not from a real column boundary.
    fn find_horizontal_split_indexed(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
    ) -> Option<(Vec<usize>, Vec<usize>)> {
        let (split_x, left, right) = self.find_horizontal_split_with_x(all_spans, indices)?;
        // A cut through a band of prose that fills the whole region -- nothing above or
        // below it to peel (`partition_indexed_depth` peels when there is) -- is no column
        // split at all. ~keep
        if prose_band_split(all_spans, indices, split_x).is_some_and(|parts| parts.len() == 1) {
            return None;
        }
        Some((left, right))
    }

    /// `find_horizontal_split_indexed`, with the cut's x. ~keep
    fn find_horizontal_split_with_x(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
    ) -> Option<(f32, Vec<usize>, Vec<usize>)> {
        let profile = self.horizontal_projection_indexed(all_spans, indices)?;

        // A valley whose cut leaves either side narrower than a column is no gutter
        // but the region's own edge: the projection counts each span's ink as 0.45 em per
        // character, so a column of long lines "ends" well before its real right edge, and
        // with a stray header number past it (Soluble p6: `12 (2026) 100361` at x 507-558)
        // that shoulder is an interior valley wider than the real 16 pt gutter. Only the
        // widest valley used to be tried; its cut was refused for width, the page got no
        // column cut and was read by y, the table note line by line into the right column's
        // prose. When the page has a column gutter of its own, such valleys are skipped and
        // the next valley is taken -- but only if its cut lies at that gutter. Without a page
        // gutter, or with a cut elsewhere, the search ends as it always did: on a
        // single-column page or a table the next valley is a gap between cells or words, and
        // taking it tore sentences and table rows. A widest valley that makes or refuses a
        // cut on any other ground decides exactly as before. ~keep
        let valleys = self.interior_valleys(&profile);
        if valleys.is_empty() {
            let split_x = self.find_split_between_peaks(&profile)?;
            return self.column_cut_at(all_spans, indices, split_x);
        }
        let page_gutter = PAGE_GUTTER.with(|cell| cell.get());
        let mut skipped = false;
        for (vs, ve, vw) in valleys {
            if vw < self.min_valley_width {
                break;
            }
            // Deepest point within the valley run, not its midpoint
            // (GH#1763) — see `deepest_valley_point` for why. ~keep
            let x_min = profile.x_min;
            let split_is_clear = |offset: f32| {
                let x = x_min + offset;
                !indices.iter().any(|&i| {
                    let bbox = &all_spans[i].bbox;
                    bbox.left() < x && x < bbox.right()
                })
            };
            let split_x = x_min + deepest_valley_point(&profile.density, vs, ve, &split_is_clear);
            if !leaves_two_columns(all_spans, indices, split_x) {
                // Without a page gutter of its own the search ends here, as it always did.
                page_gutter?;
                skipped = true;
                continue;
            }
            if skipped && !page_gutter.is_some_and(|gutter| (split_x - gutter).abs() <= FALLBACK_GUTTER_TOLERANCE_PT) {
                return None;
            }
            return self.column_cut_at(all_spans, indices, split_x);
        }
        None
    }

    /// The column cut at `split_x`, if it passes every check a column gutter must. ~keep
    fn column_cut_at(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
        split_x: f32,
    ) -> Option<(f32, Vec<usize>, Vec<usize>)> {
        // Reject splits where either resulting sub-column would be
        // narrower than ~60 pt (about 6 body-text characters at
        // 10 pt). Without this check, XY-cut recursion sub-splits
        // a single body column into sliver sub-blocks at internal
        // whitespace valleys (paragraph indentation, justified-line
        // trailing gaps, isolated short words), turning what should
        // be a clean column-major emit of a multi-column page into
        // a band-chunked stream. PDF spec §9.4.4 mentions "natural
        // reading order" but does not mandate a
        // minimum column width; this is a descriptive heuristic —
        // a real body column holds at least ~6 characters. ~keep
        const MIN_RESULT_WIDTH_PT: f32 = 60.0;
        let mut left_x_min = f32::MAX;
        let mut left_x_max = f32::MIN;
        let mut right_x_min = f32::MAX;
        let mut right_x_max = f32::MIN;
        for &i in indices {
            let l = all_spans[i].bbox.left();
            let r = all_spans[i].bbox.right();
            if l < split_x {
                left_x_min = left_x_min.min(l);
                left_x_max = left_x_max.max(r);
            } else {
                right_x_min = right_x_min.min(l);
                right_x_max = right_x_max.max(r);
            }
        }
        let left_w = left_x_max - left_x_min;
        let right_w = right_x_max - right_x_min;
        if left_w < MIN_RESULT_WIDTH_PT || right_w < MIN_RESULT_WIDTH_PT {
            return None;
        }

        // GH#1808: assign whole LINES, not individual spans. A line whose inked spans
        // straddle `split_x` -- a figure legend or a table note broken across the gutter
        // at its own font-run boundaries -- is not column content, and partitioning it
        // span-by-span tears it in two: its own comment used to claim "the column split
        // boundary will still assign them correctly by left edge", which only holds while
        // a full-width LINE is exactly one span. `partition_lines_at` sends the whole line
        // to the side holding the majority of its inked width, so a line entirely on one
        // side partitions exactly as `left edge < split_x` already did (unaffected), and
        // only a straddling line's assignment changes. ~keep
        let (left, right) = self.partition_lines_at(all_spans, indices, split_x);

        if left.is_empty() || right.is_empty() {
            return None;
        }

        // Real column splits produce balanced partitions. A 95/5 split is
        // almost always from edge dips or stray content, not a column. ~keep
        let min_side = (indices.len() / 10).max(2);
        if left.len() < min_side || right.len() < min_side {
            return None;
        }

        // Table-row guard (PMC8025747). A genuine column gutter is a
        // vertical CORRIDOR: the left column's glyphs END before the gutter
        // and the right column's glyphs BEGIN after it, so the two sides are
        // X-disjoint. A data-table row, by contrast, starts at the left
        // margin but its cells run the FULL width of the region; partitioning
        // such rows by left edge throws the wide rows into `left` while the
        // right-hand cells (their own spans) land in `right`. Taking the cut
        // anyway slices the table's rows into shattered left/right cell groups
        // — the canonical PMC8025747 p2 failure (a prose column stacked above
        // a full-width data table), and the google_doc population-table hazard
        // the post-mortem at lines 73–101 records.
        //
        // Table-row SIGNATURE: SEVERAL left-side rows each span the ENTIRE
        // right column — their glyph content reaches past the right column's
        // far edge (`right_x_max`). A data table has MANY full-width rows (the
        // header and every data row run the whole region width), so when rows
        // are bucketed into `left` by their left edge, multiple of them blanket
        // the whole right column. By contrast:
        //   * a genuine left prose / reference column ENDS before the gutter,
        //     so its lines stop well short of `right_x_max` (never counted);
        //   * a single wide mis-split OCR line (alice) yields at most one or
        //     two straddling spans — never the recurring full-width pattern;
        //   * the single-column google_doc population table short-circuits at
        //     `is_single_column_region` and never reaches here.
        // Requiring ≥ 3 such rows is what isolates the real table from those
        // cases, so the guard is SUBTRACTIVE: it only ever REJECTS a column
        // cut that would shred a table (the recursion then falls back to a row
        // cut and reads the table row-major), never adds or reorders anything.
        //
        // `core_right` (left edge + non-whitespace-char count × ~0.5 em) is
        // used instead of `bbox.right` so trailing-whitespace / advance-width
        // bbox inflation on a real left column's last word is not mistaken for
        // a glyph crossing the gutter. `overlap_tol` (~ one body em) lets a
        // single straddling glyph slip past. ~keep
        let mut right_x_max = f32::MIN;
        let mut max_font = 0.0f32;
        for &i in &right {
            right_x_max = right_x_max.max(all_spans[i].bbox.right());
            max_font = max_font.max(all_spans[i].bbox.height.abs());
        }
        let overlap_tol = max_font.max(10.0);
        let full_width_left_rows = left
            .iter()
            .filter(|&&i| {
                let s = &all_spans[i];
                let nonws = s.text.chars().filter(|c| !c.is_whitespace()).count().max(1) as f32;
                let approx_char_width = (s.font_size * 0.45).max(2.5);
                s.bbox.left() + nonws * approx_char_width >= right_x_max - overlap_tol
            })
            .count();
        if full_width_left_rows >= 3 {
            // ≥ 3 left rows each blanket the right column ⇒ this is a table-row
            // slice, not a column gutter. Don't take the column cut; the
            // recursion falls back to a row (horizontal) split and reads the
            // table row-major. ~keep
            return None;
        }

        Some((split_x, left, right))
    }

    /// Fallback column split: find the deepest trough between the two
    /// strongest density peaks. Used when the standard valley detection
    /// fails because narrow table-cell spans partially fill the gutter.
    ///
    /// Returns the split X coordinate (absolute, not relative to x_min) if
    /// a genuine trough exists — i.e., the minimum between the peaks is ≤
    /// 50% of the weaker peak density.
    fn find_split_between_peaks(&self, profile: &ProjectionProfile) -> Option<f32> {
        let density = &profile.density;
        let n = density.len();
        if n < 3 {
            return None;
        }

        // Smooth with a small box filter (window = min_valley_width) to
        // average out individual narrow peaks before finding mass centres. ~keep
        let smooth_window = (self.min_valley_width as usize).max(3);
        let half = smooth_window / 2;

        // Smooth into a reused thread-local buffer instead of a fresh `Vec` per
        // failed-valley node. Window-mean is unchanged. (Confirmed not a source
        // of the p.692 non-determinism: the buffer is cleared+refilled to exactly
        // `n` each call and never read out of range.) ~keep
        thread_local! {
            static SMOOTH_SCRATCH: std::cell::RefCell<Vec<f32>> =
                const { std::cell::RefCell::new(Vec::new()) };
        }
        SMOOTH_SCRATCH.with(|cell| {
            let mut smoothed = cell.borrow_mut();
            smoothed.clear();
            smoothed.extend((0..n).map(|i| {
                let s = i.saturating_sub(half);
                let e = (i + half + 1).min(n);
                let sum: f32 = density[s..e].iter().sum();
                sum / (e - s) as f32
            }));

            // Find the strongest peak in each half. Use `safe_float_cmp` for
            // NaN-safe total ordering — matches the comparator used elsewhere
            // in the reading-order code so `density` sentinel values can't
            // reach a `partial_cmp` that maps them to `Equal`. ~keep
            let mid = n / 2;
            let left_peak = (0..mid).max_by(|&a, &b| crate::utils::safe_float_cmp(smoothed[a], smoothed[b]))?;
            let right_peak = (mid..n).max_by(|&a, &b| crate::utils::safe_float_cmp(smoothed[a], smoothed[b]))?;

            if smoothed[left_peak] == 0.0 || smoothed[right_peak] == 0.0 {
                return None;
            }

            let search_start = left_peak.min(right_peak) + 1;
            let search_end = left_peak.max(right_peak);
            if search_start >= search_end {
                return None;
            }

            let trough_pos =
                (search_start..search_end).min_by(|&a, &b| crate::utils::safe_float_cmp(smoothed[a], smoothed[b]))?;

            let weaker_peak = smoothed[left_peak].min(smoothed[right_peak]);
            if smoothed[trough_pos] > weaker_peak * 0.5 {
                return None;
            }

            if trough_pos < self.min_valley_width as usize || trough_pos + self.min_valley_width as usize > n {
                return None;
            }

            Some(profile.x_min + trough_pos as f32)
        })
    }

    /// Find horizontal line (Y-axis) split using index-based partitioning.
    ///
    /// Returns `(above, below)` where `above` holds spans whose rectangle
    /// edge is at larger Y (higher on page in PDF coordinates) and must be
    /// processed first in reading order. PDF Spec ISO 32000-1:2008 §8.3.2.3
    /// defines the default user-space coordinate system with origin at the
    /// lower-left corner and Y increasing upward.
    fn find_vertical_split_indexed(
        &self,
        all_spans: &[TextSpan],
        indices: &[usize],
    ) -> Option<(Vec<usize>, Vec<usize>)> {
        let profile = self.vertical_projection_indexed(all_spans, indices)?;
        let (valley_start, valley_end, valley_width) = self.find_valley(&profile)?;

        if valley_width < self.min_valley_width {
            return None;
        }

        // Deepest point within the valley run, not its midpoint (GH#1763,
        // same fix as the horizontal split — see `deepest_valley_point`). ~keep
        let y_min = profile.y_min;
        let split_is_clear = |offset: f32| {
            let y = y_min + offset;
            !indices.iter().any(|&i| {
                let bbox = &all_spans[i].bbox;
                bbox.top() < y && y < bbox.bottom()
            })
        };
        let split_y = y_min + deepest_valley_point(&profile.density, valley_start, valley_end, &split_is_clear);

        // `Rect::top()` returns `self.y`, the SMALLER Y coordinate of the
        // normalized rectangle — the method name follows a screen-coordinate
        // convention (Y grows downward) but PDF user space has Y growing
        // upward, so in PDF terms `bbox.top()` is actually the LOWER edge of
        // the glyph's bounding box. The predicate `bbox.top() >= split_y`
        // therefore classifies a span into `above` only when its *lowest*
        // point is already above the split line, i.e. the entire span sits
        // above the cut. Since `split_y` is the midpoint of a horizontal
        // projection valley (an empty band by construction), spans should
        // not straddle it in practice -- and since GH#1763 the chosen point is
        // additionally checked against the real span extents, because a zero in
        // the profile does not by itself mean no glyphs are there. Any span that
        // still straddles (e.g. a tall header glyph whose ascenders dip into the
        // valley) falls into `below`. ~keep
        let (above, below): (Vec<usize>, Vec<usize>) =
            indices.iter().partition(|&&i| all_spans[i].bbox.top() >= split_y);

        if above.is_empty() || below.is_empty() {
            return None;
        }

        // Row (vertical) splits legitimately produce singleton top
        // partitions for lone headers/titles, so we accept down to 1
        // span per side. The column (horizontal) split is stricter since
        // single-span columns are almost always spurious. ~keep
        let min_side = (indices.len() / 10).max(1);
        if above.len() < min_side || below.len() < min_side {
            return None;
        }

        Some((above, below))
    }

    /// Calculate horizontal projection profile from indexed spans.
    fn horizontal_projection_indexed(&self, all_spans: &[TextSpan], indices: &[usize]) -> Option<ProjectionProfile> {
        if indices.is_empty() {
            return None;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        let mut y_min = f32::MAX;
        let mut y_max = f32::MIN;

        for &i in indices {
            let span = &all_spans[i];
            x_min = x_min.min(span.bbox.left());
            x_max = x_max.max(span.bbox.right());
            y_min = y_min.min(span.bbox.top());
            y_max = y_max.max(span.bbox.bottom());
        }

        let width = (x_max - x_min).ceil() as usize;
        if width > MAX_PROJECTION_SIZE {
            tracing::warn!(
                width,
                max = MAX_PROJECTION_SIZE,
                "horizontal projection width exceeds MAX_PROJECTION_SIZE, skipping region (degenerate CTM?)"
            );
            return None;
        }
        let mut density = vec![0.0; width];

        // Text extractors frequently over-estimate span bbox widths
        // (trailing whitespace, stretched advance widths). That makes a
        // full-width projection falsely fill the inter-column gutter on
        // multi-column pages. We project each span's TEXT CORE footprint
        // anchored to its LEFT edge (where glyphs actually start), with
        // length proportional to character count. The left edge is
        // reliable; the right edge is not.
        //
        // Additionally, spans whose core width exceeds 55% of the region
        // width are full-width elements (section headers, figure captions,
        // table titles) that span both columns. Including them fills the
        // inter-column gutter in the density array and prevents valley
        // detection. They are excluded from the projection; the column
        // split boundary will still assign them correctly by left edge. ~keep
        let region_width = (x_max - x_min).max(1.0);
        for &i in indices {
            let span = &all_spans[i];
            let height = span.bbox.bottom() - span.bbox.top();
            let char_count = span.text.chars().filter(|c| !c.is_whitespace()).count().max(1);
            // 0.45em per char is a reasonable average across common PDF
            // fonts (Helvetica/Times/Arial at body size) and narrower
            // than the 0.5em advance used for monospace. ~keep
            let approx_char_width = (span.font_size * 0.45).max(2.5);
            let core_width = char_count as f32 * approx_char_width;
            let span_width = span.bbox.right() - span.bbox.left();
            if span_width > region_width * 0.55 {
                continue;
            }
            // Skip isolated single-character/digit spans (table cell values
            // like 'G', 'T', '1', 'A') that scatter across the full X range
            // and fill the column gutter in the density profile. Body text
            // spans always contain multiple characters. ~keep
            if char_count < 2 {
                continue;
            }
            let core_left = span.bbox.left();
            let core_right = (core_left + core_width).min(span.bbox.right());
            let x_start = (core_left - x_min).max(0.0).ceil() as usize;
            let x_end = (core_right - x_min).ceil() as usize;

            for j in x_start..x_end.min(width) {
                density[j] += height;
            }
        }

        Some(ProjectionProfile { density, x_min, y_min })
    }

    /// Calculate vertical projection profile from indexed spans.
    fn vertical_projection_indexed(&self, all_spans: &[TextSpan], indices: &[usize]) -> Option<ProjectionProfile> {
        if indices.is_empty() {
            return None;
        }

        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        let mut y_min = f32::MAX;
        let mut y_max = f32::MIN;

        for &i in indices {
            let span = &all_spans[i];
            x_min = x_min.min(span.bbox.left());
            x_max = x_max.max(span.bbox.right());
            y_min = y_min.min(span.bbox.top());
            y_max = y_max.max(span.bbox.bottom());
        }

        let height = (y_max - y_min).ceil() as usize;
        if height > MAX_PROJECTION_SIZE {
            tracing::warn!(
                height,
                max = MAX_PROJECTION_SIZE,
                "vertical projection height exceeds MAX_PROJECTION_SIZE, skipping region (degenerate CTM?)"
            );
            return None;
        }
        let mut density = vec![0.0; height];

        for &i in indices {
            let span = &all_spans[i];
            let y_start = (span.bbox.top() - y_min).max(0.0).ceil() as usize;
            let y_end = (span.bbox.bottom() - y_min).ceil() as usize;
            let w = span.bbox.right() - span.bbox.left();

            for j in y_start..y_end.min(height) {
                density[j] += w;
            }
        }

        Some(ProjectionProfile { density, x_min, y_min })
    }

    /// Find the widest valley (white space gap) in projection profile.
    ///
    /// Only considers INTERIOR valleys — gaps sandwiched between two
    /// non-empty regions. Leading/trailing empty bands (margin space
    /// outside the actual content extent) are ignored; they represent
    /// page margins, not column gutters, and picking them would produce
    /// meaningless splits.
    fn interior_valleys(&self, profile: &ProjectionProfile) -> Vec<(usize, usize, f32)> {
        if profile.density.is_empty() {
            return Vec::new();
        }

        let peak = profile.density.iter().copied().fold(0.0, f32::max);

        if peak == 0.0 {
            return Vec::new();
        }

        let (Some(first_nonzero), Some(last_nonzero)) = (
            profile.density.iter().position(|&d| d > 0.0),
            profile.density.iter().rposition(|&d| d > 0.0),
        ) else {
            return Vec::new();
        };

        let threshold = peak * self.valley_threshold;
        let mut valleys = Vec::new();
        let mut in_valley = false;
        let mut valley_start = 0;

        for (i, &density) in profile.density.iter().enumerate() {
            if density < threshold {
                if !in_valley {
                    valley_start = i;
                    in_valley = true;
                }
            } else if in_valley {
                valleys.push((valley_start, i));
                in_valley = false;
            }
        }

        if in_valley {
            valleys.push((valley_start, profile.density.len()));
        }

        // Merge adjacent interior valley segments separated by a narrow
        // bridge (≤ half the minimum valley width). A callout box or small
        // figure positioned in the column gutter creates a density bump
        // that splits what should be a single valley into two fragments.
        // Bridging re-joins them so the gap is still recognised as a
        // column boundary. ~keep
        let bridge_limit = (self.min_valley_width / 2.0).ceil() as usize;
        let interior: Vec<(usize, usize)> = valleys
            .into_iter()
            .filter(|&(start, end)| start > first_nonzero && end <= last_nonzero + 1)
            .collect();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(interior.len());
        for seg in interior {
            if let Some(last) = merged.last_mut()
                && seg.0 <= last.1 + bridge_limit
            {
                last.1 = last.1.max(seg.1);
                continue;
            }
            merged.push(seg);
        }
        // Widest first; among equals the LATER valley first, the one `max_by` picks, so
        // `find_valley` is unchanged. ~keep
        let mut valleys: Vec<(usize, usize, f32)> = merged
            .into_iter()
            .map(|(start, end)| (start, end, (end - start) as f32))
            .collect();
        valleys.sort_by(|a, b| crate::utils::safe_float_cmp(b.2, a.2).then(b.0.cmp(&a.0)));
        valleys
    }

    /// The widest interior valley (see [`Self::interior_valleys`]). ~keep
    fn find_valley(&self, profile: &ProjectionProfile) -> Option<(usize, usize, f32)> {
        self.interior_valleys(profile).into_iter().next()
    }

    /// Test-only wrapper exposing `deepest_valley_point` (a free function)
    /// as an associated fn so tests can call it the same way as the other
    /// `#[cfg(test)]` wrappers in this file.
    #[cfg(test)]
    fn deepest_point_wrapper(density: &[f32], start: usize, end: usize) -> f32 {
        deepest_valley_point(density, start, end, &|_| true)
    }

    /// As [`Self::deepest_point_wrapper`], but with the span-straddle check the real
    /// callers supply, so a test can pin that a candidate cutting a span is rejected.
    #[cfg(test)]
    fn deepest_point_wrapper_checked(
        density: &[f32],
        start: usize,
        end: usize,
        split_is_clear: &dyn Fn(f32) -> bool,
    ) -> f32 {
        deepest_valley_point(density, start, end, split_is_clear)
    }

    /// Old (pre-GH#1763) split-point formula, kept only so the fixed
    /// behaviour can be asserted against what the bug used to produce. ~keep
    #[cfg(test)]
    fn legacy_valley_midpoint(start: usize, end: usize) -> f32 {
        (start + end) as f32 / 2.0
    }

    /// Test-only wrapper for horizontal projection on a contiguous slice.
    #[cfg(test)]
    fn horizontal_projection(&self, spans: &[TextSpan]) -> Option<ProjectionProfile> {
        let indices: Vec<usize> = (0..spans.len()).collect();
        self.horizontal_projection_indexed(spans, &indices)
    }

    /// Test-only wrapper for vertical projection on a contiguous slice.
    #[cfg(test)]
    fn vertical_projection(&self, spans: &[TextSpan]) -> Option<ProjectionProfile> {
        let indices: Vec<usize> = (0..spans.len()).collect();
        self.vertical_projection_indexed(spans, &indices)
    }

    /// Sort spans in reading order (top-to-bottom, left-to-right).
    #[cfg(test)]
    fn sort_spans<'a>(&self, spans: &'a [TextSpan]) -> Vec<&'a TextSpan> {
        let mut sorted: Vec<_> = spans.iter().collect();

        sorted.sort_by(|a, b| {
            let y_cmp = crate::utils::safe_float_cmp(b.bbox.top(), a.bbox.top());
            if y_cmp != std::cmp::Ordering::Equal {
                return y_cmp;
            }
            crate::utils::safe_float_cmp(a.bbox.left(), b.bbox.left())
        });

        sorted
    }

    /// Sort indices in reading order (top-to-bottom, left-to-right).
    ///
    /// Uses [`crate::utils::row_aware_span_cmp`]'s row-banded baseline
    /// comparator rather than a strict `bbox.top()` sort (GH#1600). A
    /// subscript or superscript run shares its base run's baseline
    /// (`bbox.y`) to within a fraction of a point but is drawn in a
    /// visibly smaller font, so its `top()` (`y + height`) differs from
    /// the base run's by roughly the height difference — several points,
    /// comfortably more than any OTHER same-row cell's `top()` gap. A
    /// strict `top()` sort therefore treats the subscript as a separate,
    /// lower "line" and can insert an unrelated same-row cell between a
    /// base glyph and its own subscript. `row_aware_span_cmp` quantizes Y
    /// into `ROW_BAND_TOLERANCE_PT`-wide bands before comparing, so runs
    /// sharing a baseline band stay ordered by X regardless of height. ~keep
    fn sort_indices(&self, all_spans: &[TextSpan], indices: &[usize]) -> Vec<usize> {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_by(|&a, &b| {
            crate::utils::row_aware_span_cmp(
                all_spans[a].bbox.y,
                all_spans[a].bbox.x,
                all_spans[b].bbox.y,
                all_spans[b].bbox.x,
            )
        });
        sorted
    }
}

/// Internal projection profile representation.
struct ProjectionProfile {
    /// Density values (height or width accumulated per bin)
    density: Vec<f32>,

    /// Origin coordinates
    x_min: f32,
    y_min: f32,
}

impl ReadingOrderStrategy for XYCutStrategy {
    fn apply(&self, spans: Vec<TextSpan>, context: &ReadingOrderContext) -> Result<Vec<OrderedTextSpan>> {
        // Detects multi-line heading runs and routes the
        // partition through synthetic-span space so the splitter treats
        // each wrapped heading as a single atomic block. When no
        // headings are found we use the original index-only path that
        // avoids span clones during recursion. ~keep
        let _gutter = PageGutterScope::set(context.column_gutter);
        let heading_runs = self.find_heading_runs(&spans, context.column_gutter);

        let index_groups: Vec<Vec<usize>> = if heading_runs.is_empty() {
            let indices: Vec<usize> = (0..spans.len()).collect();
            self.partition_indexed(&spans, &indices)
        } else {
            let (synthetic, synthetic_origin) = self.synthesize_for_partition(&spans, &heading_runs);
            let synth_indices: Vec<usize> = (0..synthetic.len()).collect();
            let synth_groups = self.partition_indexed(&synthetic, &synth_indices);
            // Project synthetic-space groups back to ORIGINAL-span
            // indices (so the move-out below works on the input Vec). ~keep
            synth_groups
                .into_iter()
                .map(|group| {
                    let mut out = Vec::with_capacity(group.len());
                    for synth_idx in group {
                        out.extend(synthetic_origin[synth_idx].iter().copied());
                    }
                    out
                })
                .collect()
        };

        // Build result — moves spans out by index (no extra clone) ~keep
        let mut ordered = Vec::with_capacity(spans.len());
        // Convert spans to indexable storage for O(1) moves ~keep
        let mut span_slots: Vec<Option<TextSpan>> = spans.into_iter().map(Some).collect();
        let mut order_index = 0usize;

        for (group_idx, group) in index_groups.iter().enumerate() {
            for &i in group {
                if let Some(span) = span_slots[i].take() {
                    ordered.push(
                        OrderedTextSpan::with_info(span, order_index, ReadingOrderInfo::xycut()).with_group(group_idx),
                    );
                    order_index += 1;
                }
            }
        }

        Ok(ordered)
    }

    fn name(&self) -> &'static str {
        "XYCutStrategy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn make_span(x: f32, y: f32, width: f32, height: f32) -> TextSpan {
        make_span_text(x, y, width, height, "test", 12.0)
    }

    /// Like make_span but with realistic body-text density (~72 non-whitespace chars
    /// at 12pt, matching a full Letter-width column). Used when is_single_column_region
    /// must correctly identify a wide single-column page as not multi-column.
    fn make_body_span(x: f32, y: f32, width: f32, height: f32) -> TextSpan {
        // 72 non-whitespace characters at 12pt → core_width = 72 × 5.4 = 388.8pt
        // which is 83% of a 468pt column — enough to pass the 80% dense check. ~keep
        let text = "abcdefghijklmnopqrstuvwxyz".repeat(3); // 78 non-whitespace chars ~keep
        make_span_text(x, y, width, height, &text, 12.0)
    }

    fn make_span_text(x: f32, y: f32, width: f32, height: f32, text: &str, font_size: f32) -> TextSpan {
        use crate::layout::{Color, FontWeight};

        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: text.to_string(),
            bbox: Rect::new(x, y, width, height),
            font_size,
            font_name: "Arial".to_string(),
            font_weight: FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: Color { r: 0.0, g: 0.0, b: 0.0 },
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    #[test]
    fn test_single_column_no_split() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span(10.0, 100.0, 50.0, 10.0),
            make_span(10.0, 85.0, 50.0, 10.0),
            make_span(10.0, 70.0, 50.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans, None);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
    }

    /// Realistic A4/Letter single-column page: 60 lines of body text,
    /// 14pt leading, one paragraph gap (30pt) mid-page. Only one body
    /// column exists, so XY-Cut must return exactly one group and
    /// preserve top-to-bottom reading order. A density-dip split at the
    /// paragraph gap would fragment the page and non-monotonically
    /// interleave paragraph contents.
    #[test]
    fn test_single_column_body_text_no_fragmentation() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();
        let line_height = 12.0;
        let leading = 14.0;
        let left = 72.0;
        let right = 540.0;
        let width = right - left;
        let mut y = 720.0;
        for i in 0..60 {
            // Insert a paragraph gap in the middle (30pt, larger than min_valley_width=15pt) ~keep
            if i == 30 {
                y -= 30.0;
            }
            // Use realistic body text density (78 non-whitespace chars at 12pt) so
            // is_single_column_region correctly classifies the region as single-column. ~keep
            spans.push(make_body_span(left, y, width, line_height));
            y -= leading;
        }

        let groups = strategy.partition_region(&spans, None);
        assert_eq!(
            groups.len(),
            1,
            "single-column body text must not be split by XY-Cut (got {} groups)",
            groups.len()
        );
        assert_eq!(groups[0].len(), 60, "all 60 spans must be preserved");

        let mut last_y = f32::MAX;
        for s in &groups[0] {
            assert!(
                s.bbox.top() <= last_y + 0.01,
                "reading order must be top-to-bottom: {} > {}",
                s.bbox.top(),
                last_y
            );
            last_y = s.bbox.top();
        }
    }

    /// After a vertical (row) split, the partition at higher Y (top of
    /// page in PDF coords) must be processed first in reading order so
    /// that header content appears before body content.
    #[test]
    fn test_vertical_split_preserves_top_to_bottom_order() {
        use crate::pipeline::reading_order::{ReadingOrderContext, ReadingOrderStrategy};

        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let make = |text: &str, x: f32, y: f32, w: f32| {
            let mut s = make_span(x, y, w, 12.0);
            s.text = text.to_string();
            s
        };
        // Two columns at y ∈ {200, 180, 160} (body), header at y=400.
        // Horizontal split will find the column gutter first; within each
        // column the header must still come out first in reading order. ~keep
        let spans = vec![
            make("HEADER LEFT", 50.0, 400.0, 200.0),
            make("HEADER RIGHT", 300.0, 400.0, 200.0),
            make("body-L1", 50.0, 200.0, 150.0),
            make("body-R1", 300.0, 200.0, 150.0),
            make("body-L2", 50.0, 180.0, 150.0),
            make("body-R2", 300.0, 180.0, 150.0),
        ];
        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).unwrap();

        let texts: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();
        assert!(
            texts[0].contains("HEADER"),
            "expected HEADER first, got sequence {:?}",
            texts
        );
    }

    /// Single-column page with a tall header band ("Title" or "Chapter
    /// heading") at the top. XY-Cut may validly split the header from
    /// the body (vertical Y-split) but must not further split the body
    /// into per-paragraph chunks.
    #[test]
    fn test_single_column_with_header_at_most_two_groups() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();

        spans.push(make_span(72.0, 750.0, 468.0, 24.0));

        let mut y = 670.0;
        for _ in 0..40 {
            spans.push(make_span(72.0, y, 468.0, 12.0));
            y -= 14.0;
        }

        let groups = strategy.partition_region(&spans, None);
        assert!(
            groups.len() <= 2,
            "single-column with header should produce at most 2 groups, got {}",
            groups.len()
        );
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, 41);
    }

    #[test]
    fn test_two_column_split() {
        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let spans = vec![
            make_span(10.0, 100.0, 50.0, 10.0),
            make_span(10.0, 85.0, 50.0, 10.0),
            make_span(100.0, 100.0, 50.0, 10.0),
            make_span(100.0, 85.0, 50.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans, None);
        assert!(!groups.is_empty(), "Expected at least 1 group");
        let total_spans: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total_spans, 4, "Expected all 4 spans to be preserved");
    }

    /// GH#1600. A subscript run (shorter height, baseline dropped a
    /// fraction of a point below its base run) must sort immediately after
    /// its base run, not after a sibling cell in the next column whose
    /// `top()` happens to land between them.
    ///
    /// Geometry lifted from the reporter's reproducer (`Q`/`HE`/`GJ`
    /// row): base "Q" at y=612.13 h=11.59 (top=623.72), subscript "HE" at
    /// y=611.50 h=7.34 (top=618.84 — baseline only 0.63pt below the base,
    /// but top() differs by ~4.9pt because subscript glyphs are drawn in a
    /// visibly smaller font), unit cell "GJ" in the next column at
    /// y=612.13 h=11.59 (top=623.72, tied with the base). Sorting by
    /// `top()` descending places GJ (tied top, lower x than nothing to its
    /// left) ahead of HE, tearing "QHE" into "Q" ... "HE" with "GJ" wedged
    /// between them. All three share one baseline band (`ROW_BAND_TOLERANCE_PT`
    /// = 3.0pt covers the 0.63pt baseline gap easily), so a baseline-aware,
    /// row-banded comparator keeps Q and HE adjacent. ~keep
    #[test]
    fn test_subscript_sorts_immediately_after_base_not_after_next_column() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(206.74, 612.13, 8.20, 11.59, "Q", 11.59),
            make_span_text(212.76, 611.50, 6.83, 7.34, "HE", 7.34),
            make_span_text(253.33, 612.13, 12.08, 11.59, "GJ", 11.59),
        ];

        let groups = strategy.partition_region(&spans, None);
        let texts: Vec<&str> = groups.iter().flatten().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["Q", "HE", "GJ"],
            "subscript HE must stay adjacent to base Q, ahead of the next column's GJ"
        );
    }

    /// GH#1600 negative control. Two ordinary body-text lines at a real
    /// line-height apart (14pt — typical single-spaced 12pt body leading)
    /// must NOT be treated as one row band and interleaved by X, even
    /// though they land in the same tiny (`n < min_spans_for_split`)
    /// region that reaches `sort_indices`. `ROW_BAND_TOLERANCE_PT` is
    /// 3.0pt; a 14pt gap is 4.6x that, so the two lines fall into
    /// different bands and stay in top-to-bottom, row-major order. This
    /// guards the fix above from overreaching: the row-band tolerance is
    /// narrow enough to keep a subscript with its base (0.63pt baseline
    /// gap) without also merging two genuinely separate lines whose
    /// columns would otherwise look identical to the subscript case (each
    /// side narrower than `MIN_RESULT_WIDTH_PT`, so no column split is
    /// found and both lines land in the same `sort_indices` call). ~keep
    #[test]
    fn test_row_band_does_not_merge_two_distinct_lines() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(10.0, 200.0, 50.0, 10.0, "A1", 10.0),
            make_span_text(100.0, 200.0, 50.0, 10.0, "A2", 10.0),
            make_span_text(10.0, 186.0, 50.0, 10.0, "B1", 10.0),
            make_span_text(100.0, 186.0, 50.0, 10.0, "B2", 10.0),
        ];

        let groups = strategy.partition_region(&spans, None);
        let texts: Vec<&str> = groups.iter().flatten().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["A1", "A2", "B1", "B2"],
            "two distinct 14pt-apart lines must stay in row-major order, not interleave by X"
        );
    }

    #[test]
    fn test_three_column_layout() {
        let strategy = XYCutStrategy::new();
        // Realistic column widths (≥ 60 pt per column, ≥ 6 body chars at
        // 10 pt — find_horizontal_split rejects narrower splits since
        // body columns are never sliver-wide). ~keep
        let spans = vec![
            make_span(10.0, 100.0, 100.0, 10.0),
            make_span(10.0, 85.0, 100.0, 10.0),
            make_span(180.0, 100.0, 100.0, 10.0),
            make_span(180.0, 85.0, 100.0, 10.0),
            make_span(350.0, 100.0, 100.0, 10.0),
            make_span(350.0, 85.0, 100.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans, None);
        assert!(groups.len() >= 2, "Expected at least 2 groups, got {}", groups.len());
    }

    #[test]
    fn test_small_region_no_split() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 50.0, 10.0)];

        let groups = strategy.partition_region(&spans, None);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 1);
    }

    #[test]
    fn test_sort_order() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span(100.0, 70.0, 50.0, 10.0),
            make_span(10.0, 100.0, 50.0, 10.0),
            make_span(100.0, 100.0, 50.0, 10.0),
            make_span(10.0, 70.0, 50.0, 10.0),
        ];

        let sorted = strategy.sort_spans(&spans);

        assert_eq!(sorted[0].bbox.top(), 100.0);
        assert_eq!(sorted[0].bbox.left(), 10.0);
        assert_eq!(sorted[1].bbox.top(), 100.0);
        assert_eq!(sorted[1].bbox.left(), 100.0);
    }

    #[test]
    fn test_horizontal_projection() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 30.0, 10.0), make_span(100.0, 100.0, 30.0, 10.0)];

        if let Some(profile) = strategy.horizontal_projection(&spans) {
            assert!(!profile.density.is_empty());
            assert!(profile.density.len() >= 120);

            let gap_start = 30;
            let gap_end = 90;
            if gap_end <= profile.density.len() {
                let gap_region = &profile.density[gap_start..gap_end];
                let gap_density: f32 = gap_region.iter().sum();
                assert!(gap_density < 1.0);
            }
        }
    }

    #[test]
    fn test_vertical_projection() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 50.0, 20.0), make_span(10.0, 50.0, 50.0, 20.0)];

        if let Some(profile) = strategy.vertical_projection(&spans) {
            assert!(!profile.density.is_empty());
            assert!(profile.density.len() > 50);
        }
    }

    #[test]
    fn test_narrow_gap_rejected() {
        let strategy = XYCutStrategy::new();
        let spans = vec![make_span(10.0, 100.0, 30.0, 10.0), make_span(45.0, 100.0, 30.0, 10.0)];

        let groups = strategy.partition_region(&spans, None);
        assert_eq!(groups.len(), 1);
    }

    /// Regression test for Bug 2: degenerate CTM places spans at ~100 trillion PDF points.
    /// horizontal_projection_indexed must return None instead of attempting a
    /// ~100-trillion-element vec allocation (which triggers handle_alloc_error → abort).
    #[test]
    fn test_degenerate_ctm_horizontal_projection_returns_none() {
        let strategy = XYCutStrategy::new();
        // Observed crash coordinate: 99_992_777_785_344 PDF points on a ~3968-point page. ~keep
        let degenerate_x: f32 = 99_992_777_785_344.0;
        let spans = vec![
            make_span(10.0, 100.0, 30.0, 10.0),
            make_span(degenerate_x, 100.0, 30.0, 10.0),
        ];

        let result = strategy.horizontal_projection(&spans);
        assert!(
            result.is_none(),
            "expected None for projection spanning ~100 trillion points, got Some"
        );
    }

    /// Vertical projection must also return None for degenerate CTM y-coordinates.
    #[test]
    fn test_degenerate_ctm_vertical_projection_returns_none() {
        let strategy = XYCutStrategy::new();
        let degenerate_y: f32 = 99_992_777_785_344.0;
        let spans = vec![
            make_span(10.0, 100.0, 30.0, 10.0),
            make_span(10.0, degenerate_y, 30.0, 10.0),
        ];

        let result = strategy.vertical_projection(&spans);
        assert!(
            result.is_none(),
            "expected None for projection spanning ~100 trillion points, got Some"
        );
    }

    /// A CENTERED title/subtitle/byline block (each line
    /// centered, scattered leftmost edges) must NOT be split into
    /// per-word "columns". The centered "Quarterly Inventory Review"
    /// title (3 large words at the same Y with wide gaps) plus centered
    /// subtitle/byline previously aligned accidentally into fake columns,
    /// scrambling reading order. The centered-block guard must keep the
    /// whole block as ONE group so the title line stays intact.
    #[test]
    fn test_issue1_centered_title_block_not_split_into_columns() {
        let strategy = XYCutStrategy::new();
        // Centered title (y=612, fs=28), subtitle (y=572), byline (y=532).
        // Leftmost edges scattered: 145 / 185 / 210 (centered, not columnar). ~keep
        let spans = vec![
            make_span_text(145.0, 612.0, 115.0, 28.0, "Quarterly", 28.0),
            make_span_text(300.0, 612.0, 115.0, 28.0, "Inventory", 28.0),
            make_span_text(430.0, 612.0, 92.0, 28.0, "Review", 28.0),
            make_span_text(185.0, 572.0, 40.0, 14.0, "Spring", 14.0),
            make_span_text(238.0, 572.0, 31.0, 14.0, "2025", 14.0),
            make_span_text(300.0, 572.0, 70.0, 14.0, "Distribution", 14.0),
            make_span_text(210.0, 532.0, 45.0, 10.0, "Northwind", 10.0),
            make_span_text(290.0, 532.0, 34.0, 10.0, "Traders", 10.0),
        ];
        let groups = strategy.partition_region(&spans, None);
        assert_eq!(
            groups.len(),
            1,
            "centered title block must stay one group, got {} groups",
            groups.len()
        );
        let g0: Vec<&str> = groups[0].iter().map(|s| s.text.as_str()).collect();
        let qi = g0.iter().position(|t| *t == "Quarterly").unwrap();
        let ii = g0.iter().position(|t| *t == "Inventory").unwrap();
        let ri = g0.iter().position(|t| *t == "Review").unwrap();
        assert!(qi < ii && ii < ri, "title words out of order: {:?}", g0);
    }

    /// XYCut must assign distinct group_id values to spans in different
    /// spatial partitions so that converters can keep each column's content
    /// contiguous instead of interleaving by Y-coordinate.
    #[test]
    fn test_xycut_group_id_two_column_layout() {
        use crate::pipeline::reading_order::{ReadingOrderContext, ReadingOrderStrategy};

        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let make = |text: &str, x: f32, y: f32, w: f32| {
            let mut s = make_span(x, y, w, 12.0);
            s.text = text.to_string();
            s
        };
        let spans = vec![
            make("Description", 50.0, 100.0, 150.0),
            make("Amount", 400.0, 100.0, 150.0),
            make("Widget A", 50.0, 120.0, 150.0),
            make("$150.00", 400.0, 120.0, 150.0),
            make("Widget B", 50.0, 140.0, 150.0),
            make("Discount", 400.0, 140.0, 150.0),
            make("$25.00", 400.0, 160.0, 150.0),
        ];

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).unwrap();

        assert!(
            ordered.iter().all(|s| s.group_id.is_some()),
            "all spans should have group_id set by XYCut"
        );

        let left_groups: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() < 300.0)
            .map(|s| s.group_id.unwrap())
            .collect();
        let right_groups: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() >= 300.0)
            .map(|s| s.group_id.unwrap())
            .collect();

        assert!(
            left_groups.windows(2).all(|w| w[0] == w[1]),
            "left column spans should share the same group_id: {:?}",
            left_groups
        );
        assert!(
            right_groups.windows(2).all(|w| w[0] == w[1]),
            "right column spans should share the same group_id: {:?}",
            right_groups
        );

        assert_ne!(
            left_groups[0], right_groups[0],
            "left and right columns should have different group_ids"
        );

        let left_orders: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() < 300.0)
            .map(|s| s.reading_order)
            .collect();
        let right_orders: Vec<usize> = ordered
            .iter()
            .filter(|s| s.span.bbox.left() >= 300.0)
            .map(|s| s.reading_order)
            .collect();
        let left_max = *left_orders.iter().max().unwrap();
        let right_min = *right_orders.iter().min().unwrap();
        let left_min = *left_orders.iter().min().unwrap();
        let right_max = *right_orders.iter().max().unwrap();
        assert!(
            left_max < right_min || right_max < left_min,
            "columns must be contiguous in reading order: left={:?} right={:?}",
            left_orders,
            right_orders
        );
    }

    /// Plain-text rendering must keep group_id-separated columns as
    /// contiguous blocks, not interleave them by Y-coordinate.
    #[test]
    fn test_group_id_plain_text_no_interleave() {
        use crate::pipeline::reading_order::{ReadingOrderContext, ReadingOrderStrategy};

        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        let make = |text: &str, x: f32, y: f32, w: f32| {
            let mut s = make_span(x, y, w, 12.0);
            s.text = text.to_string();
            s
        };
        let spans = vec![
            make("Description", 50.0, 100.0, 150.0),
            make("Amount", 400.0, 100.0, 150.0),
            make("Widget A", 50.0, 120.0, 150.0),
            make("$150.00", 400.0, 120.0, 150.0),
            make("Widget B", 50.0, 140.0, 150.0),
            make("Discount", 400.0, 140.0, 150.0),
            make("$25.00", 400.0, 160.0, 150.0),
        ];

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).unwrap();

        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        for expected in ["Description", "Amount", "Widget A", "$150.00"] {
            assert!(order.contains(&expected), "missing {expected:?}: {order:?}");
        }

        // Each group_id-separated column must occupy one contiguous run of the
        // reading order. Interleaving them by Y — the defect this guards — would
        // scatter each column's spans through the other's. ~keep
        let run_is_contiguous = |members: &[&str]| {
            let mut indices: Vec<usize> = members
                .iter()
                .map(|t| order.iter().position(|s| s == t).expect("span present"))
                .collect();
            indices.sort_unstable();
            indices.windows(2).all(|w| w[1] == w[0] + 1)
        };
        assert!(
            run_is_contiguous(&["Description", "Widget A", "Widget B"]),
            "left column must be one contiguous run: {order:?}"
        );
        assert!(
            run_is_contiguous(&["Amount", "$150.00", "Discount", "$25.00"]),
            "right column must be one contiguous run: {order:?}"
        );
    }

    /// Builder for a bold heading span at a given font size. Used by the
    /// fix-543 tests to construct the "bold/large-font run spanning ≥ 2
    /// lines" shape the pre-partition heading lock must catch.
    fn make_bold_span(x: f32, y: f32, width: f32, text: &str, font_size: f32) -> TextSpan {
        use crate::layout::FontWeight;
        let mut s = make_span_text(x, y, width, font_size, text, font_size);
        s.font_weight = FontWeight::Bold;
        s
    }

    /// fix-543 unit: `find_heading_runs` must detect a 2-line bold
    /// heading whose wrapped tail line sits below the first line with
    /// matching X-extent. Single-line bold spans or paragraph-gap
    /// shapes must NOT be returned.
    #[test]
    fn find_heading_runs_detects_2_line_bold_heading() {
        let strategy = XYCutStrategy::new();

        // Body baseline (12pt regular) — establishes the median. ~keep
        let mut spans = Vec::new();
        let body_left = 72.0;
        let body_width = 200.0;
        let mut y = 720.0;
        for _ in 0..10 {
            spans.push(make_body_span(body_left, y, body_width, 12.0));
            y -= 14.0;
        }

        spans.push(make_bold_span(body_left, 500.0, 180.0, "2.3 Performance and", 14.0));
        spans.push(make_bold_span(
            body_left,
            484.0,
            180.0,
            "Advantages of Vari-linear Network",
            14.0,
        ));

        let runs = strategy.find_heading_runs(&spans, None);
        assert_eq!(runs.len(), 1, "expected exactly one heading run, got {runs:?}");
        assert_eq!(
            runs[0].span_indices.len(),
            2,
            "expected the run to cover both heading lines"
        );

        // A LONE bold span (no second line) must NOT be locked: that
        // case is a single-line heading that XY-cut already handles. ~keep
        let mut spans_single = vec![make_body_span(body_left, 720.0, body_width, 12.0); 5];
        spans_single.push(make_bold_span(body_left, 500.0, 180.0, "Lone Heading", 14.0));
        let runs_single = strategy.find_heading_runs(&spans_single, None);
        assert!(
            runs_single.is_empty(),
            "single-line bold runs must not produce a HeadingRun"
        );
    }

    /// fix-543 unit: the canonical repro shape — left-column 2-line
    /// bold heading whose wrapped tail line Y-overlaps right-column
    /// dense content (table caption + rows). Pre-fix, line 2 of the
    /// heading was bucketed into the RIGHT block; post-fix the lock
    /// keeps both heading lines in the LEFT block, adjacent to the
    /// left-column body paragraph.
    #[test]
    fn partition_keeps_heading_in_left_block() {
        let strategy = XYCutStrategy::new();

        let left_col_x = 72.0_f32;
        let right_col_x = 362.0_f32;
        let col_width = 260.0_f32;

        let mut spans = Vec::new();

        // Left column: 2-line bold heading at Y=500/484, then 8 body
        // lines below at Y=460..360 (so the body paragraph anchors the
        // left block in reading order). ~keep
        spans.push(make_bold_span(left_col_x, 500.0, 180.0, "2.3 Performance and", 14.0));
        spans.push(make_bold_span(
            left_col_x,
            484.0,
            220.0,
            "Advantages of Vari-linear Network",
            14.0,
        ));
        let mut y = 460.0_f32;
        for _ in 0..8 {
            spans.push(make_body_span(left_col_x, y, col_width, 12.0));
            y -= 14.0;
        }

        // Right column: dense table-caption-style content that
        // Y-overlaps the heading's second line (Y=484). The
        // pre-fix block-assignment step pulled the heading's tail
        // into THIS column because the geometry was alone in
        // deciding bucket membership. ~keep
        spans.push(make_body_span(right_col_x, 500.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 484.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 468.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 452.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 436.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 420.0, col_width, 12.0));
        spans.push(make_body_span(right_col_x, 404.0, col_width, 12.0));

        let groups = strategy.partition_region(&spans, None);

        let heading_first_group = groups
            .iter()
            .position(|g| g.iter().any(|s| s.text.contains("2.3 Performance and")))
            .expect("heading line 1 must land in some group");
        let heading_second_group = groups
            .iter()
            .position(|g| g.iter().any(|s| s.text.contains("Advantages of Vari-linear Network")))
            .expect("heading line 2 must land in some group");

        assert_eq!(
            heading_first_group, heading_second_group,
            "both heading lines must end up in the SAME block — pre-fix \
             they split across left/right column blocks"
        );

        let group = &groups[heading_first_group];
        for s in group {
            assert!(
                s.bbox.left() < right_col_x,
                "heading + body group must stay in the LEFT column; \
                 stray span at x={} (right_col starts at {}): {:?}",
                s.bbox.left(),
                right_col_x,
                s.text
            );
        }
    }

    /// fix-543 unit: a wrapped heading's tail line must stay in its own
    /// column block. Pre-fix the tail was bucketed into the right-hand
    /// column and ordered after that column's body, orphaning it from the
    /// heading it belongs to.
    #[test]
    fn wrapped_heading_tail_stays_in_its_column_block() {
        use crate::pipeline::reading_order::ReadingOrderContext;

        let strategy = XYCutStrategy::new();

        let left_col_x = 72.0_f32;
        let right_col_x = 362.0_f32;
        let col_width = 260.0_f32;

        let mut spans = Vec::new();
        spans.push(make_bold_span(left_col_x, 500.0, 180.0, "Performance and", 14.0));
        spans.push(make_bold_span(
            left_col_x,
            484.0,
            220.0,
            "Advantages of Vari-linear Network",
            14.0,
        ));
        let mut y = 460.0_f32;
        for _ in 0..6 {
            spans.push(make_body_span(left_col_x, y, col_width, 12.0));
            y -= 14.0;
        }
        for ky in [500.0, 484.0, 468.0, 452.0, 436.0, 420.0] {
            spans.push(make_body_span(right_col_x, ky, col_width, 12.0));
        }

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).expect("apply");
        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        // Both heading halves must appear, and BOTH must precede any
        // right-column content. Pre-fix, the wrapped-heading tail was
        // bucketed into the right-column block and emitted AFTER the
        // right column's body, then promoted to a fresh heading level
        // by `heading_level_ratio` (since it lost its body
        // continuation) — that's the phantom `### …` in the wrong
        // location. Post-fix the lock keeps both heading lines in the
        // left block adjacent to each other. ~keep
        let pos_first = order
            .iter()
            .position(|t| t.contains("Performance and"))
            .expect("heading line 1 must appear in reading order");
        let pos_second = order
            .iter()
            .position(|t| t.contains("Advantages of Vari-linear Network"))
            .expect("heading line 2 must appear in reading order");

        assert_eq!(
            pos_second,
            pos_first + 1,
            "the wrapped heading's two lines must stay adjacent: {order:?}"
        );
        // Both heading lines belong at the very top of the left-column
        // block, not floating somewhere after the right-column body.
        // (Pre-fix the orphan tail landed deep into the document.) ~keep
        let cap = ((order.len() as f32) * 0.30) as usize;
        assert!(
            pos_second < cap.max(2),
            "heading-line-2 ordered late — likely the pre-fix \
             orphan-in-wrong-column behaviour. pos_second={pos_second}, \
             cap={cap}, order={order:?}"
        );
    }

    /// GH#1738: mid-X between the left column's right edge (237.56) and the
    /// right column's left edge (312.60) on the reproducer's page 1.
    const GH1738_GUTTER_X: f32 = 275.08;

    /// The GH#1738 reproducer's page 1 as a fixture, with the right column's
    /// caption parameterised. `(10.0, 810.40)` is the measured original; the
    /// GH#1757 negative control re-runs the same page with the caption at the
    /// heading's own size and on its row.
    ///
    /// Geometry transcribed verbatim from `PdfDocument::extract_spans` on
    /// page 1 of the GH#1738 reproducer (a two-column A4 page: a bold
    /// numbered heading opening the top of the left column, a bold
    /// caption at the top of the right column, and a body underneath
    /// each) — `x`, `y` (top-origin) and `font_size` are the measured
    /// values, and `y` decreases top-to-bottom exactly like every other
    /// `y` in this file's `dense_two_column_*` fixtures. No position is
    /// invented. ~keep
    fn gh1738_page(caption_font_size: f32, caption_top: f32) -> Vec<TextSpan> {
        let kern_space = |x: f32, y: f32, width: f32, font_size: f32| {
            let mut s = make_span_text(x, y, width, font_size, " ", font_size);
            s.offset_semantic = true;
            s
        };
        let body = |x: f32, y: f32, width: f32, text: &str| make_span_text(x, y, width, 8.3, text, 8.3);

        #[rustfmt::skip]
        let spans = vec![
            make_bold_span(30.07, 809.09, 7.51, "3.", 9.0),
            kern_space(37.58, 809.09, 0.28, 9.0),
            make_bold_span(312.60, caption_top, 129.00, "Fig. 6. Branderdruk (P1-P2)", caption_font_size),
            make_bold_span(49.54, 809.09, 188.02, "INSTRUCTIES VOOR DE GASTECHNISCHE ", 9.0),
            make_bold_span(49.54, 799.39, 67.66, "INSTALLATEUR", 9.0),
            make_bold_span(30.07, 782.02, 12.51, "3.1", 9.0),
            kern_space(42.58, 782.02, 0.28, 9.0),
            make_bold_span(49.54, 782.02, 158.68, "GASAANSLUITING EN INSTALLATIE.", 9.0),
            body(30.10, 764.00, 6.92, "1."),
            body(42.80, 764.00, 224.46, "Werk altijd volgens de laatste eisen van de geldende normen"),
            body(42.80, 755.00, 116.33, "en de plaatselijke voorschriften."),
            body(30.10, 737.00, 6.92, "2."),
            body(42.80, 737.00, 202.33, "Plaats bij te verwachten vuil in het gas bij voorkeur een"),
            body(42.80, 728.00, 78.05, "gaszeef in de leiding."),
            body(30.10, 710.00, 6.92, "3."),
            body(42.80, 710.00, 225.82, "Als het gasblok op dichtheid wordt gecontroleerd, gebeurt dat"),
            body(42.80, 701.00, 189.36, "met een druk van ten hoogste 500 mm waterkolom."),
            body(30.10, 683.00, 6.92, "4."),
            body(42.80, 683.00, 223.45, "De fabrieksafstelling van de tweetrapsregeling bedraagt 21,6"),
            body(42.80, 674.00, 224.47, "kW voor warm water en 14 kW voor de verwarming. Voor het"),
            body(42.80, 665.00, 219.31, "aanpassen van de verwarmingsinstelling, zie het kopje over"),
            body(42.80, 656.00, 199.91, "de tweetrapsregeling. De branderdruk is de uitlaatdruk"),
            body(42.80, 647.00, 220.75, "gemeten ten opzichte van de vuurhaarddruk. Voor de plaats"),
            body(42.80, 638.00, 213.45, "van de meetnippels, zie fig. 5. Sluit voor het meten van de"),
            body(42.80, 629.00, 210.56, "branderdruk de slangen van de drukverschilmeter aan op"),
            body(42.80, 620.00, 196.75, "beide meetnippels. In fig. 6 is het nominale vermogen"),
            body(42.80, 611.00, 113.12, "uitgezet tegen de branderdruk."),
            make_bold_span(30.10, 519.00, 135.65, "Fig. 5. Branderdrukinstelling", 10.0),
            make_bold_span(312.60, 518.40, 79.52, "Tweetrapsregeling", 9.0),
            body(312.60, 500.30, 227.13, "Als de verwarmingsinstallatie meer of minder vermogen nodig"),
            body(312.60, 491.30, 234.65, "heeft dan de fabrieksafstelling van 14 kW, kan de capaciteit van"),
            body(312.60, 482.30, 223.06, "de ketel met de tweetrapsregeling worden aangepast aan de"),
            body(312.60, 473.30, 220.77, "installatie. Op de fabriek wordt de hoogste belasting voor de"),
            body(312.60, 464.30, 234.97, "warmwatervoorziening afgesteld op het nominale vermogen. De"),
            body(312.60, 455.30, 197.63, "tweetrapsregeling (zie fig. 7) wordt als volgt ingesteld:"),
            body(312.60, 446.30, 87.90, "a. Spoel onbekrachtigd."),
            body(322.50, 437.30, 225.84, "Lage belasting instellen met de zeskante stelschroef A (let op"),
            body(322.50, 428.30, 128.87, "dat deze vrij ligt van stelschroef B)."),
            body(312.60, 419.30, 76.36, "b. Spoel bekrachtigd"),
            body(322.50, 410.30, 205.03, "Hoogste belasting controleren en zo nodig afstellen met"),
            body(322.50, 401.30, 50.31, "stelschroef B."),
            body(322.50, 392.30, 191.66, "Stift tegenhouden met een inbussleutel van 2,5 mm."),
            body(312.60, 383.30, 94.84, "c. Stelschroef A aflakken."),
            body(312.60, 374.30, 115.11, "d. Branderdrukken controleren."),
            make_bold_span(312.60, 293.20, 120.10, "Fig. 7. Tweetrapsregeling", 10.0),
        ];
        spans
    }

    /// GH#1738: a numbered heading whose producer set the marker and title
    /// in ONE `TJ` array with a kern for the tab (`[(3.)-1329.5(Title )] TJ`)
    /// must not absorb a right-column caption that Y-overlaps its first
    /// line. The kern becomes a space-only span that is always
    /// `FontWeight::Normal` (see `extractors/text/advance.rs`), which used
    /// to break `find_heading_runs`'s clustering right at the
    /// marker/title boundary: the marker ("3.") was left out of the run
    /// while the wrapped title ("…GASTECHNISCHE" / "INSTALLATEUR")
    /// formed a run on its own, whose narrower union bbox no longer
    /// covered the marker's column position and let the caption's span
    /// land between the run's two original lines once expanded. ~keep
    #[test]
    fn gh1738_kern_tab_heading_does_not_absorb_other_column_caption() {
        use crate::pipeline::reading_order::ReadingOrderContext;

        let strategy = XYCutStrategy::new();
        let spans = gh1738_page(10.0, 810.40);

        let context = ReadingOrderContext::new();
        let ordered = strategy.apply(spans, &context).expect("apply");
        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        let pos_marker = order
            .iter()
            .position(|&t| t == "3.")
            .expect("the heading marker must appear in reading order");
        let pos_title = order
            .iter()
            .position(|&t| t == "INSTRUCTIES VOOR DE GASTECHNISCHE ")
            .expect("the heading title must appear in reading order");
        let pos_wrap = order
            .iter()
            .position(|&t| t == "INSTALLATEUR")
            .expect("the heading's wrapped second line must appear in reading order");
        let pos_caption = order
            .iter()
            .position(|&t| t == "Fig. 6. Branderdruk (P1-P2)")
            .expect("the other column's caption must appear in reading order");

        assert_eq!(
            pos_title,
            pos_marker + 1,
            "the marker and title must stay adjacent: {order:?}"
        );
        assert_eq!(
            pos_wrap,
            pos_title + 1,
            "the wrapped second line must stay adjacent to the title, not have \
             the other column's caption spliced in between: {order:?}"
        );
        assert!(
            pos_caption < pos_marker || pos_caption > pos_wrap,
            "the other column's caption must not land inside the heading run \
             (marker={pos_marker}, title={pos_title}, wrap={pos_wrap}, \
             caption={pos_caption}): {order:?}"
        );
    }

    /// GH#1757 left column, line 1: the chapter title following the `3.` marker.
    const GH1757_TITLE: &str = "INSTRUKTIES VOOR DE GASTECHNISCHE ";
    /// GH#1757 right column: the section heading that hijacked the run.
    const GH1757_RIGHT_HEADING: &str = "3.2 VERBRANDINGSGASAFVOER EN LUCHTTOEVOER";
    /// GH#1757: mid-X of the gutter between the page's two detected columns
    /// (`Detected 2 columns: [(34.622, 276.310), (276.310, 560.031)]`).
    const GH1757_GUTTER_X: f32 = 276.31;

    /// GH#1757: page 4 of the installation manual (A4, 595 × 842). A numbered
    /// chapter heading wraps to a second line at the top of the LEFT column
    /// while the RIGHT column opens with a section heading in the same face,
    /// 0.25 pt lower. `right_top`, `right_bold` and `right_font_size` select
    /// the rows of the issue's variants table.
    ///
    /// Heading positions are the reporter's measured values, in PDF
    /// coordinates (`y` = box top, decreasing down the page) — the left
    /// heading's two lines at 804.93 / 794.13 and the right heading at
    /// 804.68 are the content stream's own `Tm` operands. Body lines carry
    /// the manual's measured leading with paraphrased text, exactly as the
    /// reporter's reproducer sets them. ~keep
    fn gh1757_wrapped_heading_over_two_columns(
        right_top: f32,
        right_bold: bool,
        right_font_size: f32,
    ) -> Vec<TextSpan> {
        let kern_space = |x: f32, y: f32, width: f32, font_size: f32| {
            let mut s = make_span_text(x, y, width, font_size, " ", font_size);
            s.offset_semantic = true;
            s
        };
        let body = |x: f32, y: f32, width: f32, text: &str| make_span_text(x, y, width, 8.3, text, 8.3);

        let right_heading = if right_bold {
            make_bold_span(322.7, right_top, 237.1, GH1757_RIGHT_HEADING, right_font_size)
        } else {
            make_span_text(
                322.7,
                right_top,
                237.1,
                right_font_size,
                GH1757_RIGHT_HEADING,
                right_font_size,
            )
        };

        // The producer draws the whole RIGHT column as one text object and the
        // whole LEFT column as a second, so the right heading precedes the
        // left heading in span order. ~keep
        let mut spans = vec![right_heading];
        for (i, text) in [
            "Het afvoersysteem en de uitmonding voldoen aan de geldende",
            "norm voor gesloten toestellen met ventilator in een",
            "opstellingsruimte. De afvoerleiding mag op afschot naar het",
            "toestel liggen, want bij de toegestane lengte en de",
            "voorgeschreven mantel ontstaat er geen condens. Een",
            "doorvoer naar buiten ligt op een afschot van ten minste vijf",
            "millimeter per meter naar buiten, zodat er geen regen in kan",
            "lopen. Het toestel vangt zelf geen condens of regenwater op.",
        ]
        .into_iter()
        .enumerate()
        {
            spans.push(body(322.7, 784.8 - (i as f32) * 10.5, 237.3, text));
        }
        spans.push(make_bold_span(
            322.7,
            668.0,
            149.8,
            "3.2.1 AANSLUITING OP DE KETEL",
            9.0,
        ));
        for (i, text) in [
            "Het toestel wordt geleverd met een aansluitset voor een",
            "bovenaansluiting met twee stompen van rond 80 mm. Op",
            "bestelling is een set voor een achteraansluiting leverbaar,",
            "eveneens met twee stompen van rond 80 mm.",
        ]
        .into_iter()
        .enumerate()
        {
            spans.push(body(322.7, 648.2 - (i as f32) * 10.5, 237.3, text));
        }

        spans.push(make_bold_span(34.6, 804.93, 7.5, "3.", 9.0));
        spans.push(kern_space(42.14, 804.93, 0.28, 9.0));
        spans.push(make_bold_span(55.9, 804.93, 185.2, GH1757_TITLE, 9.0));
        spans.push(make_bold_span(55.9, 794.13, 67.6, "INSTALLATEUR", 9.0));
        spans.push(make_bold_span(
            34.6,
            773.3,
            179.9,
            "3.1 GASAANSLUITING EN INSTALLATIE.",
            9.0,
        ));
        for (i, text) in [
            "1. Werk altijd volgens de laatste eisen en de plaatselijke",
            "voorschriften.",
            "2. Plaats bij te verwachten vuil in het gas bij voorkeur een",
            "gaszeef.",
            "3. Een dichtheidscontrole van het gasblok gebeurt met een druk",
            "van ten hoogste 500 mm waterkolom.",
            "4. Heeft de installatie minder vermogen nodig dan de",
            "fabrieksafstelling, dan kan de branderdruk naar de gewenste",
            "capaciteit worden aangepast volgens figuur 3.",
        ]
        .into_iter()
        .enumerate()
        {
            spans.push(body(34.6, 753.5 - (i as f32) * 10.5, 236.8, text));
        }
        spans
    }

    /// Texts of the spans backing each detected heading run, in run order.
    fn heading_run_texts<'a>(spans: &'a [TextSpan], runs: &[HeadingRun]) -> Vec<Vec<&'a str>> {
        runs.iter()
            .map(|r| r.span_indices.iter().map(|&i| spans[i].text.as_str()).collect())
            .collect()
    }

    /// GH#1757: the left column's wrapped chapter heading must be locked as
    /// ONE run even though the right column's section heading sorts between
    /// its two lines. The right heading sits across the detected gutter, so
    /// it is neither folded into the run nor allowed to close it.
    #[test]
    fn wrapped_heading_run_survives_other_column_heading_gh1757() {
        let strategy = XYCutStrategy::new();
        let spans = gh1757_wrapped_heading_over_two_columns(804.68, true, 9.0);

        let runs = strategy.find_heading_runs(&spans, Some(GH1757_GUTTER_X));
        let texts = heading_run_texts(&spans, &runs);

        assert_eq!(
            texts,
            vec![vec!["3.", GH1757_TITLE, "INSTALLATEUR"]],
            "expected exactly one locked heading run covering the marker, the \
             title and its wrapped continuation line"
        );
        assert!(
            !texts.iter().flatten().any(|t| *t == GH1757_RIGHT_HEADING),
            "the other column's heading must not be part of any heading run: {texts:?}"
        );
    }

    /// GH#1757: the skip is gated entirely on a known gutter. A caller with
    /// none must get the pre-GH#1757 behaviour verbatim — the far span folds
    /// in and the run is lost — so that pages nobody classified as
    /// multi-column are untouched.
    ///
    /// A width-based stand-in for the gutter was built and measured, and it
    /// is why this test asserts the defect rather than the fix: on one corpus
    /// document it fired 1385 times with a median gap of 82 pt, half of them
    /// at or above the 79 pt gap of GH#1757's own gutter. No threshold
    /// separates a cross-gutter gap from an in-line one without a gutter to
    /// measure against. The fix is to give every output path the gutter (see
    /// `PdfDocument::detect_column_gutter`), not to guess at one here. ~keep
    #[test]
    fn heading_run_fold_is_unchanged_without_a_known_gutter_gh1757() {
        let strategy = XYCutStrategy::new();
        let spans = gh1757_wrapped_heading_over_two_columns(804.68, true, 9.0);

        assert!(
            strategy.find_heading_runs(&spans, None).is_empty(),
            "with no gutter the far span must still fold in, exactly as before"
        );
    }

    /// GH#1757 at the entry points that actually reach the output lenses.
    ///
    /// `find_heading_runs` runs about a dozen times per page from several
    /// call sites; `postprocess_spans` is only one of them, and threading the
    /// gutter through it alone left the reproducer welded because the text
    /// and markdown lenses read the ordering produced by `partition_region`
    /// and by `apply`. Both must honour the gutter, so both are asserted
    /// here: the marker, its title and the wrapped continuation line come out
    /// adjacent and ahead of the other column's heading. ~keep
    #[test]
    fn output_path_entry_points_honour_the_gutter_gh1757() {
        use crate::pipeline::reading_order::ReadingOrderContext;

        let strategy = XYCutStrategy::new();
        let spans = gh1757_wrapped_heading_over_two_columns(804.68, true, 9.0);

        let partition_groups = strategy.partition_region(&spans, Some(GH1757_GUTTER_X));
        let from_partition: Vec<&str> = partition_groups.iter().flatten().map(|s| s.text.as_str()).collect();

        let context = ReadingOrderContext::new().with_column_gutter(GH1757_GUTTER_X);
        let applied = strategy.apply(spans.clone(), &context).expect("apply");
        let from_apply: Vec<&str> = applied.iter().map(|o| o.span.text.as_str()).collect();

        for (label, order) in [("partition_region", &from_partition), ("apply", &from_apply)] {
            let position = |needle: &str| {
                order
                    .iter()
                    .position(|t| *t == needle)
                    .unwrap_or_else(|| panic!("{label}: {needle:?} missing from {order:?}"))
            };
            let marker = position("3.");
            let title = position(GH1757_TITLE);
            let wrap = position("INSTALLATEUR");
            let other = position(GH1757_RIGHT_HEADING);

            assert_eq!(
                title,
                marker + 1,
                "{label}: marker and title must stay adjacent: {order:?}"
            );
            assert_eq!(
                wrap,
                title + 1,
                "{label}: the wrapped line must follow the title: {order:?}"
            );
            assert!(
                other > wrap,
                "{label}: the other column's heading must not precede or split the \
                 heading run (marker={marker}, title={title}, wrap={wrap}, \
                 other={other}): {order:?}"
            );
        }
    }

    /// GH#1757 control (the reproducer's page 2): the right column's heading
    /// 1.5 pt ABOVE line 1 sorts before it and falls outside the 1 pt
    /// same-line window, so the left heading already locks correctly today.
    /// It must keep doing so.
    #[test]
    fn wrapped_heading_run_intact_when_other_column_heading_sits_higher_gh1757() {
        let strategy = XYCutStrategy::new();
        let spans = gh1757_wrapped_heading_over_two_columns(806.43, true, 9.0);

        assert_eq!(
            heading_run_texts(&spans, &strategy.find_heading_runs(&spans, Some(GH1757_GUTTER_X))),
            vec![vec!["3.", GH1757_TITLE, "INSTALLATEUR"]],
            "the control page's heading run must stay intact"
        );
    }

    /// GH#1757 variants table, the two rows that weld on the stock build:
    /// the right heading on exactly the same baseline as line 1, and the
    /// right heading one point larger. Both must leave the left column's run
    /// whole.
    ///
    /// Asserting the run's exact membership matters here: "the right heading
    /// is in no run" is also true of the defect, which produces no runs at
    /// all. ~keep
    #[test]
    fn other_column_bold_heading_variants_keep_the_run_gh1757() {
        let strategy = XYCutStrategy::new();

        for (right_top, right_font_size, label) in [
            (804.9295_f32, 9.0_f32, "same baseline"),
            (804.68_f32, 10.0_f32, "10 pt"),
        ] {
            let spans = gh1757_wrapped_heading_over_two_columns(right_top, true, right_font_size);
            let texts = heading_run_texts(&spans, &strategy.find_heading_runs(&spans, Some(GH1757_GUTTER_X)));
            assert_eq!(
                texts,
                vec![vec!["3.", GH1757_TITLE, "INSTALLATEUR"]],
                "variant '{label}': the left column's heading run must stay whole"
            );
        }
    }

    /// GH#1757 variants table, the regular-face row. It never welded — a
    /// 9 pt regular span on a 8.3 pt body page is not heading-like, so
    /// clustering rejects it before any same-line test. Green before and
    /// after the fix; pinned so a later widening of `is_heading_like` cannot
    /// quietly turn the other column's opening line into run material.
    ///
    /// The left column's run is still lost on this variant — a far
    /// non-heading-like span BREAKS the run rather than being skipped, which
    /// is the issue's option 3 and is not addressed here. ~keep
    #[test]
    fn other_column_regular_face_line_never_joins_the_run_gh1757() {
        let strategy = XYCutStrategy::new();
        let spans = gh1757_wrapped_heading_over_two_columns(804.68, false, 9.0);

        let texts = heading_run_texts(&spans, &strategy.find_heading_runs(&spans, Some(GH1757_GUTTER_X)));

        assert!(
            !texts.iter().flatten().any(|t| *t == GH1757_RIGHT_HEADING),
            "a regular-face line in the other column must not join a heading run: {texts:?}"
        );
    }

    /// GH#1757 negative control for GH#1738. That test's caption clears
    /// `size_ok` (10 pt against the heading's 9 pt) AND sits 1.31 pt above
    /// the heading row, outside the 1 pt same-line window — two independent
    /// reasons it never reached the same-line fold. Put it at the heading's
    /// own size and 0.25 pt below its row, as GH#1757's page has it, and it
    /// takes exactly the GH#1757 path: the stock build folds it in, the
    /// wrapped line then fails the indent test against it, and the run is
    /// lost. `apply` then welds the whole top row into one line.
    #[test]
    fn gh1738_equal_size_caption_on_the_heading_row_is_not_absorbed_gh1757() {
        use crate::pipeline::reading_order::ReadingOrderContext;

        let strategy = XYCutStrategy::new();
        let spans = gh1738_page(9.0, 808.84);

        assert_eq!(
            heading_run_texts(&spans, &strategy.find_heading_runs(&spans, Some(GH1738_GUTTER_X))),
            vec![vec!["3.", "INSTRUCTIES VOOR DE GASTECHNISCHE ", "INSTALLATEUR"]],
            "the caption must not be folded into the heading run"
        );

        let context = ReadingOrderContext::new().with_column_gutter(GH1738_GUTTER_X);
        let ordered = strategy.apply(spans, &context).expect("apply");
        let order: Vec<&str> = ordered.iter().map(|o| o.span.text.as_str()).collect();

        let position = |needle: &str| {
            order
                .iter()
                .position(|t| *t == needle)
                .unwrap_or_else(|| panic!("{needle:?} must appear in reading order: {order:?}"))
        };
        let pos_marker = position("3.");
        let pos_title = position("INSTRUCTIES VOOR DE GASTECHNISCHE ");
        let pos_wrap = position("INSTALLATEUR");
        let pos_caption = position("Fig. 6. Branderdruk (P1-P2)");

        assert_eq!(
            pos_title,
            pos_marker + 1,
            "the marker and title must stay adjacent: {order:?}"
        );
        assert_eq!(
            pos_wrap,
            pos_title + 1,
            "the wrapped second line must stay adjacent to the title: {order:?}"
        );
        assert!(
            pos_caption < pos_marker || pos_caption > pos_wrap,
            "an equal-size caption on the heading's own row must not land inside \
             the heading run (marker={pos_marker}, title={pos_title}, \
             wrap={pos_wrap}, caption={pos_caption}): {order:?}"
        );
    }

    /// A 2-column body where the gutter is narrower than
    /// `min_valley_width` AND the line-start cluster shape carries
    /// outlier singletons (title / caption / equation labels) so
    /// `detect_two_column_prose` bails on `clusters.len() != 2`.
    /// The narrow-gutter prose detector should catch this via
    /// gap-position clustering.
    #[test]
    fn test_narrow_gutter_prose_with_outlier_singletons() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        let mut spans = Vec::new();
        // 14 body lines with a tight gutter at x ≈ 295 (gap is
        // ~10 pt: left column ends at ~285, right column starts at
        // ~305). Each side has multiple per-word spans so the line
        // density is realistic. ~keep
        for i in 0..14 {
            let y = 600.0 - (i as f32) * 14.0;
            let left_words = ["Dwarf", "spheroidal", "galaxies", "of", "the", "Local", "Group", "are"];
            let mut x = 40.0;
            for w in left_words {
                spans.push(make_word(x, y, w));
                x += (w.chars().count() as f32 * 5.4) + 2.5;
            }
            let right_words = [
                "The",
                "Schwarzschild",
                "modeling",
                "technique",
                "offers",
                "another",
                "approach",
                "to",
            ];
            let mut x = 305.0;
            for w in right_words {
                spans.push(make_word(x, y, w));
                x += (w.chars().count() as f32 * 5.4) + 2.5;
            }
        }
        // Outlier singletons (title / caption / equation labels)
        // whose left edges don't align with either column. Under
        // detect_two_column_prose these produce extra clusters and
        // block detection — the narrow-gutter detector should still
        // catch the body via gap-position clustering. ~keep
        spans.push(make_word(145.0, 700.0, "Title text spanning"));
        spans.push(make_word(214.0, 680.0, "Caption that wraps somewhere"));
        spans.push(make_word(455.0, 670.0, "(1)"));
        spans.push(make_word(505.0, 660.0, "(2)"));

        let groups = strategy.partition_region(&spans, None);
        assert!(
            groups.len() >= 2,
            "expected at least 2 groups (column split) for narrow-gutter 2-col body \
             with outlier singletons; got {} group(s)",
            groups.len()
        );

        for (gi, g) in groups.iter().enumerate() {
            let has_left = g.iter().any(|s| s.bbox.left() < 200.0);
            let has_right = g.iter().any(|s| s.bbox.left() >= 305.0);
            assert!(
                !(has_left && has_right),
                "group {} contains spans from both columns — the column split did \
                 not separate them: {:?}",
                gi,
                g.iter().map(|s| (s.text.clone(), s.bbox.left())).collect::<Vec<_>>()
            );
        }
    }

    /// Negative: a single-column body with one large figure caption
    /// produces a strong within-line gap on the caption row but no
    /// recurring gap pattern across body lines. The narrow-gutter
    /// detector must NOT fire (would scramble reading order).
    #[test]
    fn test_narrow_gutter_prose_negative_single_col_with_caption() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        let mut spans = Vec::new();
        for i in 0..14 {
            let y = 600.0 - (i as f32) * 14.0;
            let words = [
                "This",
                "is",
                "an",
                "ordinary",
                "single",
                "column",
                "body",
                "paragraph",
                "with",
                "no",
                "interior",
                "gutter",
                "or",
                "wide",
                "gap",
            ];
            let mut x = 40.0;
            for w in words {
                spans.push(make_word(x, y, w));
                x += (w.chars().count() as f32 * 5.4) + 2.5;
            }
        }
        // One row that DOES have a within-line gap (figure caption
        // with a label on the right). This single outlier must not
        // make the page look 2-column. ~keep
        spans.push(make_word(40.0, 410.0, "Figure"));
        spans.push(make_word(80.0, 410.0, "caption"));
        spans.push(make_word(300.0, 410.0, "(continued)"));

        let groups = strategy.partition_region(&spans, None);
        // For a true single-column page, partition_region should
        // return either ONE group or a small number from row/header
        // splits — never a column split that lands left-side spans
        // in one group and right-side spans in another.
        // Count groups that contain at least one body span (x < 100): ~keep
        let body_groups = groups
            .iter()
            .filter(|g| g.iter().any(|s| s.bbox.left() < 100.0 && s.text != "Figure"))
            .count();
        assert!(
            body_groups <= 1,
            "narrow-gutter detector wrongly column-split a single-column body: \
             body spans landed in {} groups",
            body_groups
        );
    }

    /// xycut mirror: a short-verse two-column body —
    /// short tokens per column-line (`mean_chars <= 20`) but a strong
    /// balanced central gutter — must classify as `Prose` (so it gets
    /// cut) and be accepted by `detect_narrow_gutter_prose`, even though
    /// the long-line `mean_chars > 20` guard would reject it.
    #[test]
    fn test_short_verse_two_column_classified_prose_and_cut() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        // Two columns: left starts at x=40, right at x=240. Each verse
        // line carries two short, EQUAL-length 4-char tokens per side
        // (8 non-whitespace chars/side → 16 chars/line, so mean_chars
        // ≤ 20 and the long-line prose guard does NOT apply — this
        // exercises the new short-line admission arm). The left column's
        // right edge lands consistently near x≈94 and the gutter gap
        // midpoint is stable at ≈167 every line (region x≈40..≈294,
        // width≈254, gutter offset ≈0.50·width). Stable gap → high
        // corridor concentration; equal token counts → balanced char
        // mass; two tight left-column start X's (40, 72) within one
        // column → ≤ 2 left-edge clusters. Uniform token widths keep the
        // within-line gap midpoint inside the 10 pt clustering radius. ~keep
        let left_lines = [
            ["comm", "lalu"],
            ["crea", "ciel"],
            ["terr", "etai"],
            ["info", "vide"],
            ["surf", "labi"],
            ["espr", "leau"],
        ];
        let right_lines = [
            ["EtD1", "ditq"],
            ["lumi", "soit"],
            ["etla", "fut1"],
            ["Dieu", "vitq"],
            ["bonn", "ilse"],
            ["aral", "obsc"],
        ];
        let mut spans = Vec::new();
        // 24 lines total (4 verse-stanzas of 6) so the body clears the
        // ≥12 gap-bearing-line floor in detect_narrow_gutter_prose. ~keep
        for rep in 0..4 {
            for i in 0..6 {
                let y = 600.0 - ((rep * 6 + i) as f32) * 14.0;
                spans.push(make_word(40.0, y, left_lines[i][0]));
                spans.push(make_word(72.0, y, left_lines[i][1]));
                spans.push(make_word(240.0, y, right_lines[i][0]));
                spans.push(make_word(272.0, y, right_lines[i][1]));
            }
        }

        let indices: Vec<usize> = (0..spans.len()).collect();
        assert_eq!(
            strategy.classify_region_kind(&spans, &indices),
            RegionKind::Prose,
            "short-verse two-column body with a strong balanced central \
             corridor must classify as Prose despite mean_chars <= 20"
        );
        assert!(
            strategy
                .detect_narrow_gutter_prose(&spans, &indices, strategy.classify_region_kind(&spans, &indices),)
                .is_some(),
            "detect_narrow_gutter_prose must accept the routed short-verse \
             body (gutter found) so it is cut at the gutter"
        );
    }

    /// xycut mirror — negative: a short-cell multi-column
    /// numeric table (four narrow digit columns → short cells with ≥ 3
    /// left-edge clusters and scattered within-line gaps) must STILL
    /// classify as `Table` and NOT be accepted for cutting.
    #[test]
    fn test_short_cell_label_table_still_table_not_cut() {
        let strategy = XYCutStrategy::new();
        let make_word = |x: f32, y: f32, text: &str| {
            let w = (text.chars().count() as f32 * 5.4).max(3.0);
            make_span_text(x, y, w, 12.0, text, 12.0)
        };

        // A lopsided label+data table: a tiny numeric label column at
        // x=40 (a single digit, ~1 char) and a wide data column at x=100
        // (~8 chars). The within-line gutter is consistent, so the
        // corridor concentration/coverage/centre guards alone would NOT
        // reject it — but the left/right non-whitespace char balance is
        // grossly lopsided (label side ≈ 11 % of chars, well under the
        // 35 % floor), the length-independent table discriminator. A
        // genuine two-column verse body has balanced sides; this table
        // does not, so the short-line admission must reject it.
        // mean_chars ≈ 9 (≥ 8), so it does NOT fall through the
        // `mean_chars < 8 → Table` branch either — the balance check is
        // what keeps it out of Prose. ~keep
        let mut spans = Vec::new();
        let labels = ["7", "8", "9", "5", "3", "1"];
        let data = ["12345678", "23456781", "34567812", "45678123"];
        for i in 0..24 {
            let y = 600.0 - (i as f32) * 14.0;
            spans.push(make_word(40.0, y, labels[i % labels.len()]));
            spans.push(make_word(100.0, y, data[i % data.len()]));
        }

        let indices: Vec<usize> = (0..spans.len()).collect();
        assert_ne!(
            strategy.classify_region_kind(&spans, &indices),
            RegionKind::Prose,
            "lopsided label+data table must NOT be admitted as Prose \
             (left/right char mass is unbalanced — label column is tiny)"
        );
        assert!(
            strategy
                .detect_narrow_gutter_prose(&spans, &indices, strategy.classify_region_kind(&spans, &indices),)
                .is_none(),
            "detect_narrow_gutter_prose must reject the short-cell table \
             (no central-corridor Prose admission) so it is NOT cut"
        );
    }

    #[test]
    fn test_degenerate_ctm_partition_region_does_not_abort() {
        let strategy = XYCutStrategy::new();
        let degenerate_x: f32 = 99_992_777_785_344.0;
        let spans = vec![
            make_span(10.0, 100.0, 30.0, 10.0),
            make_span(10.0, 85.0, 30.0, 10.0),
            make_span(10.0, 70.0, 30.0, 10.0),
            make_span(10.0, 55.0, 30.0, 10.0),
            make_span(10.0, 40.0, 30.0, 10.0),
            make_span(degenerate_x, 100.0, 30.0, 10.0),
        ];

        let groups = strategy.partition_region(&spans, None);
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, spans.len(), "all spans must be preserved");
    }

    /// Many distinct-Y single spans is the singleton-peel pathology. With the
    /// depth cap, `partition_region` must still terminate and preserve every
    /// span (the cap falls back to a flat sort, which keeps all indices).
    #[test]
    fn test_partition_indexed_depth_guard_preserves_all_spans() {
        let mut strategy = XYCutStrategy::new();
        strategy.min_spans_for_split = 2;

        // 300 spans, each on its own Y band — deeper than MAX_PARTITION_DEPTH. ~keep
        let spans: Vec<TextSpan> = (0..300)
            .map(|i| make_span(10.0, (i as f32) * 11.0, 30.0, 10.0))
            .collect();

        let groups = strategy.partition_region(&spans, None);
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert_eq!(total, spans.len(), "depth guard must not drop spans");
    }

    /// Reproduces GH#1763: a figure-caption block (left) beside a body
    /// column (right) whose true empty gutter sits OFF the interior
    /// valley's arithmetic midpoint.
    ///
    /// Geometry (all X in points, region x_min = 0):
    ///   - `CAP1`/`CAP2` (2/3 non-ws chars, 10pt font ⇒ core width 9/13.5pt,
    ///     both left-edge 0) overlap at bin 0, giving density 20 there —
    ///     ABOVE the run's threshold (18 = 0.3 × peak 60) so `find_valley`'s
    ///     interior filter (`start > first_nonzero`) admits the run that
    ///     follows instead of treating the whole region as one leading
    ///     margin. This is the "super-threshold strip at the left content
    ///     edge" the bug fix's interior-run gate requires.
    ///   - `FIG.3.A` (bold, 14pt, 7 non-ws chars ⇒ core width 44.1pt,
    ///     left-edge 60) and `CAP4`/`CAP5` (30/25 non-ws chars, 10pt,
    ///     left-edges 150/320) are ragged sub-threshold caption content —
    ///     real ink, never above 18 density, ending at x = 342 (CAP5's
    ///     core right edge = 320 + 25×4.5 = 432.5, ceil 433 — the LAST
    ///     content before the true gutter).
    ///   - `BODY1..BODY6` (56 non-ws chars, 10pt ⇒ core width 252pt,
    ///     left-edge 500, six identical lines) set the peak: 6 × 10 = 60.
    ///
    /// The resulting horizontal-projection run below threshold spans bins
    /// [9, 500) (width 491, comfortably the widest and only interior
    /// valley — `BODY` is uniform so it contributes no valley of its
    /// own). Within that run the true empty gutter is [433, 500) (width
    /// 67) — clearly off-center: the run's own arithmetic midpoint,
    /// (9 + 500) / 2 = 254.5, lands inside `CAP4`'s span (density 10 at
    /// that x), not in the empty band.
    ///
    /// All five upstream column/prose detectors decline on this fixture
    /// before reaching the valley split, so the bug path is genuinely
    /// exercised: `detect_two_column_prose` sees 5 left-edge clusters
    /// (0, 60, 150, 320, 500 — the staggered caption starts plus the
    /// body's own), not the exactly-2 it requires; `detect_narrow_gutter_prose`
    /// declines outright (11 spans < its 24-span floor); and
    /// `is_single_column_region` returns false because no single line's
    /// extent reaches 60% of the 752pt region width. ~keep
    fn gh1763_page() -> Vec<TextSpan> {
        let cap1 = make_span_text(0.0, 740.0, 9.0, 10.0, "c1", 10.0);
        let cap2 = make_span_text(0.0, 725.0, 13.5, 10.0, "c2z", 10.0);
        let fig3 = make_bold_span(60.0, 760.0, 44.1, "FIG.3.A", 14.0);
        let cap4 = make_span_text(150.0, 705.0, 135.0, 10.0, &format!("CAP4{}", "x".repeat(26)), 10.0);
        let cap5 = make_span_text(320.0, 685.0, 112.5, 10.0, &format!("CAP5{}", "x".repeat(21)), 10.0);

        let mut spans = vec![cap1, cap2, fig3, cap4, cap5];
        for (i, y) in [655.0, 635.0, 615.0, 595.0, 575.0, 555.0].into_iter().enumerate() {
            spans.push(make_span_text(
                500.0,
                y,
                252.0,
                10.0,
                &format!("BODY{}{}", i, "x".repeat(51)),
                10.0,
            ));
        }
        spans
    }

    /// RED-then-GREEN unit test for the GH#1763 fix. Asserts the profile,
    /// the chosen valley run, and the resulting split coordinate
    /// explicitly — not just inferred from final group membership. Before
    /// the fix, `find_horizontal_split_indexed` used
    /// `legacy_valley_midpoint(vs, ve)` (254.5) here; that value falls
    /// inside `CAP4`'s span, which this test also pins down. ~keep
    /// Center of the widest zero-density sub-run in the GH#1763 fixture's
    /// valley. It falls BETWEEN two bins because that sub-run has odd
    /// width; both neighbouring bins are asserted empty below, which is
    /// the property that matters. Pinned as a constant so the split-point
    /// assertion and the density lookups that prove it lands in real empty
    /// space cannot drift apart. ~keep
    const DEEPEST_POINT: f32 = 466.5;

    /// The GH#1763 fix must be a NO-OP on a uniformly empty valley run --
    /// the ordinary case of a real column gutter. It is not enough that the
    /// new split be "close": `find_horizontal_split_indexed` feeds the value
    /// straight into a coordinate comparison, so a half-unit shift reassigns
    /// any span whose edge falls in between. An earlier revision computed the
    /// sub-run center as `start + width / 2` in `usize`, which truncates and
    /// moved the split by 0.5 on EVERY odd-width run -- silently changing the
    /// common case this fix exists to leave alone. ~keep
    /// The guard that keeps the GH#1763 relocation to pages that actually have the defect.
    /// Here the midpoint already falls in empty space, and a WIDER empty region sits
    /// off-centre. Relocating would be pointless -- the split was cutting nothing where it
    /// was -- and it is exactly this case that made the unguarded fix rewrite the reading
    /// order of 23 of 230 corpus documents, 11 of which got worse by dictionary-valid word
    /// count. The split must not move. ~keep
    /// A zero in the projection does not mean no glyphs sit there:
    /// `horizontal_projection_indexed` omits spans under two non-whitespace characters,
    /// spans wider than 55% of the region, and everything past a span's estimated text core.
    /// Seeking the deepest point walks straight into those blind spots, and the corpus showed
    /// the result -- words coming apart at single-character spans, "virgin" into "v" +
    /// "irgin". A candidate that cuts a real span must be rejected in favour of the next, and
    /// a run whose candidates all cut something must keep the midpoint. ~keep
    #[test]
    fn a_candidate_that_would_cut_a_span_is_rejected() {
        // content | gap A (narrow, clear) | content over the midpoint | gap B (wider, crossed)
        let mut density = vec![0.0f32; 40];
        for bin in (0..10).chain(18..23) {
            density[bin] = 3.0;
        }
        let (start, end) = (0usize, 40usize);
        // The midpoint must sit ON content, or the floor guard returns it before any
        // candidate is considered and this pins nothing.
        assert_eq!(density[20], 3.0, "midpoint must be on content for this test to bite");

        // Gap B (23..40, width 17) beats gap A (10..18, width 8) on width, but an invisible
        // span -- one the projection omitted -- runs straight through gap B's centre.
        let invisible_span = 28.0f32..35.0f32;
        let clear = |offset: f32| !(invisible_span.start < offset && offset < invisible_span.end);

        assert_eq!(
            XYCutStrategy::deepest_point_wrapper(&density, start, end),
            31.5,
            "unchecked, the widest gap wins and the split lands inside the hidden span"
        );
        assert_eq!(
            XYCutStrategy::deepest_point_wrapper_checked(&density, start, end, &clear),
            14.0,
            "checked, the split falls back to the narrower gap that cuts nothing"
        );
    }

    /// When every candidate would cut a span, the split must stay exactly where it was
    /// before GH#1763 -- the midpoint -- rather than picking the least-bad cut. ~keep
    #[test]
    fn a_run_whose_candidates_all_cut_something_keeps_the_midpoint() {
        let mut density = vec![0.0f32; 40];
        for bin in (0..10).chain(18..23) {
            density[bin] = 3.0;
        }
        let (start, end) = (0usize, 40usize);
        let midpoint = XYCutStrategy::legacy_valley_midpoint(start, end);
        assert_eq!(
            density[20], 3.0,
            "midpoint must be on content, or the floor guard decides this"
        );

        assert_eq!(
            XYCutStrategy::deepest_point_wrapper_checked(&density, start, end, &|_| false),
            midpoint,
            "no clear candidate means the pre-GH#1763 midpoint stands"
        );
    }

    #[test]
    fn a_split_already_falling_through_empty_space_does_not_move() {
        // content | narrow gap (holds the midpoint) | content | WIDER gap, off-centre
        let mut density = vec![0.0f32; 40];
        for bin in (0..17).chain(23..26) {
            density[bin] = 3.0;
        }
        let (start, end) = (0usize, 40usize);
        let midpoint = XYCutStrategy::legacy_valley_midpoint(start, end);
        assert_eq!(midpoint, 20.0);
        assert_eq!(density[20], 0.0, "the midpoint must start out in empty space");
        assert!(
            (26..40).len() > (17..23).len(),
            "the off-centre empty region must be the WIDER one, or this pins nothing"
        );

        assert_eq!(
            XYCutStrategy::deepest_point_wrapper(&density, start, end),
            midpoint,
            "a split already falling through empty space must stay where it is"
        );
    }

    #[test]
    fn uniform_run_split_is_unchanged_from_the_legacy_midpoint() {
        // This pins a behavioural guarantee -- a uniformly empty run splits exactly where it
        // always did -- and not the centre arithmetic, which it cannot reach: a uniform run's
        // midpoint is by definition at the density floor, so the floor guard returns first.
        // `a_candidate_that_would_cut_a_span_is_rejected` is what pins the f32 centre.
        // The runs are still sized to clear the sparse-run gate so the guarantee is delivered
        // by the floor guard rather than by short-circuiting earlier still.
        for (start, end) in [(3usize, 8usize), (3, 9), (0, 5), (0, 4), (10, 17), (2, 5), (1, 64)] {
            let density = vec![0.0f32; end];
            let share = (end - start) as f32 / end as f32;
            assert!(
                share > SPARSE_VALLEY_REGION_SHARE,
                "run [{start},{end}) is {share} of its region; the sparse gate would short-circuit it"
            );
            assert_eq!(
                XYCutStrategy::deepest_point_wrapper(&density, start, end),
                XYCutStrategy::legacy_valley_midpoint(start, end),
                "uniformly empty run [{start},{end}) must split exactly where it always did"
            );
        }
    }

    /// A below-threshold run narrow enough to BE a gutter keeps its midpoint without the
    /// candidate search running at all -- that is the case GH#1763 must not disturb, and it
    /// is most of the corpus. ~keep
    #[test]
    fn a_run_narrow_enough_to_be_a_gutter_keeps_its_midpoint() {
        // 20 empty bins in a 200-bin region: 10%, the shape of a real column gutter. An
        // off-centre single-bin dip would otherwise win the candidate search.
        let mut density = vec![5.0f32; 200];
        for bin in 90..110 {
            density[bin] = 1.0;
        }
        density[92] = 0.0;
        let (start, end) = (90usize, 110usize);
        assert!(
            ((end - start) as f32) <= density.len() as f32 * SPARSE_VALLEY_REGION_SHARE,
            "this run must be narrow enough for the gate to fire, or the test pins nothing"
        );
        assert_eq!(
            XYCutStrategy::deepest_point_wrapper(&density, start, end),
            XYCutStrategy::legacy_valley_midpoint(start, end),
            "a gutter-width run must split at its midpoint, as it always did"
        );
    }

    /// The GH#1763 reporter could not supply a reproducing PDF, but did attach the
    /// horizontal projection their page 8 actually produced under stock v1.2.7. Running
    /// the real profile is what binds this fix to the reported page rather than to a
    /// hand-built approximation of it: the synthetic `gh1763_page` fixture exercises the
    /// same mechanism, but only this asserts the reported page now splits where the
    /// reporter said it should.
    ///
    /// Their stated numbers, all reproduced below: threshold 98.03, valley run
    /// x 53.6..307.6, buggy midpoint x 180.6, target x 301.6. The valley's empty core is
    /// 12 pt wide -- UNDER `min_valley_width` (15) -- which is why the width gate must
    /// keep applying to the run as a whole and not to its core, as the issue warns. ~keep
    #[test]
    fn the_reported_gh1763_profile_now_splits_at_its_empty_core() {
        const PROFILE: &str = include_str!("../../../tests/fixtures/gh1763_horizontal_density_profile.txt");
        const X_MIN: f32 = 37.587;
        const VALLEY_THRESHOLD: f32 = 0.3;
        const MIN_VALLEY_WIDTH: f32 = 15.0;

        let density: Vec<f32> = PROFILE
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| line.parse().expect("fixture must hold one float per line"))
            .collect();
        assert_eq!(density.len(), 523, "fixture must be the reporter's full profile");

        let peak = density.iter().copied().fold(f32::MIN, f32::max);
        let threshold = VALLEY_THRESHOLD * peak;
        assert_eq!(
            (peak * 100.0).round() / 100.0,
            326.77,
            "profile peak must match the reporter's"
        );
        assert_eq!(
            (threshold * 100.0).round() / 100.0,
            98.03,
            "valley threshold must match the reporter's"
        );

        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut index = 0usize;
        while index < density.len() {
            if density[index] < threshold {
                let start = index;
                while index < density.len() && density[index] < threshold {
                    index += 1;
                }
                if start > 0 && index < density.len() {
                    runs.push((start, index));
                }
            } else {
                index += 1;
            }
        }
        let (valley_start, valley_end) = runs
            .into_iter()
            .max_by_key(|(start, end)| end - start)
            .expect("the profile must contain an interior valley");
        assert_eq!(
            (X_MIN + valley_start as f32, X_MIN + valley_end as f32),
            (53.587, 307.587),
            "widest interior valley run must be the one the reporter measured"
        );

        let run_width = (valley_end - valley_start) as f32;
        assert!(
            run_width >= MIN_VALLEY_WIDTH,
            "the width gate applies to the whole run, which passes it"
        );

        let buggy = X_MIN + XYCutStrategy::legacy_valley_midpoint(valley_start, valley_end);
        assert_eq!(buggy, 180.587, "pre-fix split must be the midpoint the reporter saw");

        let fixed = X_MIN + XYCutStrategy::deepest_point_wrapper(&density, valley_start, valley_end);
        assert_eq!(
            fixed, 301.587,
            "fixed split must be the empty core the reporter identified"
        );
        assert_eq!(
            density[(fixed - X_MIN) as usize],
            0.0,
            "the fixed split must land on a zero-density bin"
        );
    }

    #[test]
    fn find_valley_selects_the_deepest_point_not_the_midpoint_gh1763() {
        let strategy = XYCutStrategy::new();
        let spans = gh1763_page();

        let profile = strategy
            .horizontal_projection(&spans)
            .expect("non-empty span set must produce a projection profile");

        let (valley_start, valley_end, valley_width) = strategy
            .find_valley(&profile)
            .expect("a wide interior valley must be found");
        assert_eq!(
            (valley_start, valley_end, valley_width),
            (9, 500, 491.0),
            "unexpected valley run bounds"
        );

        let buggy_midpoint = XYCutStrategy::legacy_valley_midpoint(valley_start, valley_end);
        assert_eq!(buggy_midpoint, 254.5, "pre-fix formula must land inside CAP4's span");
        assert_eq!(
            profile.density[254], 10.0,
            "the pre-fix midpoint must land on real (sub-threshold) caption content, not empty space"
        );

        let deepest = XYCutStrategy::deepest_point_wrapper(&profile.density, valley_start, valley_end);
        assert_eq!(
            deepest, DEEPEST_POINT,
            "fixed split must land at the center of the widest zero-density sub-run"
        );
        assert_eq!(
            (profile.density[466], profile.density[467]),
            (0.0, 0.0),
            "the bins either side of the fixed split must be truly empty, not caption content"
        );
        assert_ne!(deepest, buggy_midpoint, "the fix must actually move the split point");
    }

    /// Behavioral counterpart: `find_horizontal_split_indexed` (the real
    /// caller, not a hand-rolled reimplementation) must partition the
    /// GH#1763 fixture so every caption fragment (`CAP1`, `CAP2`,
    /// `FIG.3.A`, `CAP4`, `CAP5`) stays on the left and every body line
    /// (`BODY*`) stays on the right — pre-fix, `CAP5` crossed into the
    /// body side because the buggy midpoint (254.5) sits to the LEFT of
    /// CAP5's own left edge (320). ~keep
    #[test]
    fn find_horizontal_split_indexed_keeps_caption_fragments_together_gh1763() {
        let strategy = XYCutStrategy::new();
        let spans = gh1763_page();
        let indices: Vec<usize> = (0..spans.len()).collect();

        let (left, right) = strategy
            .find_horizontal_split_indexed(&spans, &indices)
            .expect("a valid column split must be found");

        assert_eq!(
            left,
            vec![0, 1, 2, 3, 4],
            "left side must hold exactly the 5 caption fragments"
        );
        assert_eq!(
            right,
            vec![5, 6, 7, 8, 9, 10],
            "right side must hold exactly the 6 body lines"
        );
    }

    /// Integration-level counterpart via the public `partition_region`
    /// entry point. Column purity — no caption fragment sharing a final
    /// group with any body line, and vice versa — is asserted rather
    /// than "same group_id for all 5 caption fragments", because the
    /// sparse, widely-spaced caption fragments legitimately subdivide
    /// further under recursion (each such sub-split is rejected by
    /// `MIN_RESULT_WIDTH_PT`, so in practice they land in one group, but
    /// the invariant that must hold regardless is column purity). ~keep
    #[test]
    fn gh1763_caption_fragments_never_bleed_into_the_body_column() {
        let strategy = XYCutStrategy::new();
        let spans = gh1763_page();

        let groups = strategy.partition_region(&spans, None);

        let group_of = |text: &str| -> usize {
            groups
                .iter()
                .position(|g| g.iter().any(|s| s.text == text))
                .unwrap_or_else(|| panic!("{text} missing from output: {groups:?}"))
        };

        let caption_texts = ["c1", "c2z", "FIG.3.A"];
        let caption_groups: Vec<usize> = caption_texts.iter().map(|t| group_of(t)).collect();
        let cap4_group = groups
            .iter()
            .position(|g| g.iter().any(|s| s.text.starts_with("CAP4")))
            .expect("CAP4 fragment missing");
        let cap5_group = groups
            .iter()
            .position(|g| g.iter().any(|s| s.text.starts_with("CAP5")))
            .expect("CAP5 fragment missing");
        let body_groups: Vec<usize> = (0..6)
            .map(|i| {
                groups
                    .iter()
                    .position(|g| g.iter().any(|s| s.text.starts_with(&format!("BODY{i}"))))
                    .unwrap_or_else(|| panic!("BODY{i} missing from output: {groups:?}"))
            })
            .collect();

        for &cg in caption_groups.iter().chain([&cap4_group, &cap5_group]) {
            assert!(
                !body_groups.contains(&cg),
                "a caption fragment must never share a group with body content: {groups:?}"
            );
        }
        // The specific manifestation of the bug: CAP5 must stay with FIG.3.A,
        // not fall into the body group. ~keep
        assert_eq!(
            cap5_group, caption_groups[2],
            "CAP5 must group with FIG.3.A, not drift to the body column: {groups:?}"
        );
    }

    /// GH#1808 reproducer: a two-column region (left/right lines at y=300/285/270) sits
    /// above an 8-line figure legend that runs the full region width (x 40..300). Each
    /// legend line is written as 3 font-run fragments with small (5pt) inter-run gaps --
    /// the shape a `Tm`+`TJ`-per-run producer emits for a caption. Region width is 260pt
    /// (40..300), so a legend line's own 260pt extent is exactly full width.
    ///
    /// the fragments carry text of a realistic length for their width (~0.5 em per
    /// character); `ink_right` cuts back a bbox its text cannot fill. ~keep
    fn gh1808_two_columns_over_torn_legend_spans() -> Vec<TextSpan> {
        let mut spans = Vec::new();
        for &y in &[300.0, 285.0, 270.0] {
            spans.push(make_span_text(40.0, y, 100.0, 10.0, "left column line", 10.0));
            spans.push(make_span_text(200.0, y, 100.0, 10.0, "right column line", 10.0));
        }
        for row in 0..8 {
            let y = 150.0 - row as f32 * 12.0;
            spans.push(make_span_text(40.0, y, 60.0, 7.17, "LEG-A lorem ipsum do", 7.17)); // 40..100
            spans.push(make_span_text(
                105.0,
                y,
                95.0,
                7.17,
                "LEG-B lorem ipsum dolor sit a",
                7.17,
            )); // 105..200, gap 5
            spans.push(make_span_text(
                205.0,
                y,
                95.0,
                7.17,
                "LEG-C lorem ipsum dolor sit a",
                7.17,
            )); // 205..300, gap 5
        }
        spans
    }

    /// RED-then-GREEN unit test for GH#1808's peel (`peel_full_width_line_bands`). Before
    /// the fix (no peeling at all) this method did not exist and every legend fragment
    /// reached `find_horizontal_split_indexed` ungrouped, where a column cut near x=150-190
    /// would tear each legend line at its own font-run boundary. Neutering the fix (see the
    /// commit message for the verbatim pre-fix failure) makes this method return `None`;
    /// with the fix, the legend is peeled into its own band, entirely separate from the two
    /// column lines, and every legend line's 3 fragments stay together in that one band. ~keep
    #[test]
    fn peel_full_width_line_bands_keeps_a_torn_legend_whole_gh1808() {
        let strategy = XYCutStrategy::new();
        let spans = gh1808_two_columns_over_torn_legend_spans();
        let indices: Vec<usize> = (0..spans.len()).collect();

        let bands = strategy
            .peel_full_width_line_bands(&spans, &indices)
            .expect("an 8-line full-width legend must be peeled off as its own band(s)");

        assert_eq!(bands.len(), 2, "expected exactly [column band, legend band]: {bands:?}");
        let column_band = &bands[0];
        let legend_band = &bands[1];
        assert_eq!(
            column_band.len(),
            6,
            "the two columns' 6 lines must form the first band"
        );
        assert_eq!(
            legend_band.len(),
            24,
            "all 8 legend lines' 24 fragments must form the second band"
        );

        // No legend line's 3 fragments may be split across bands: each of LEG-A/B/C's 8
        // occurrences must appear only in `legend_band`, never in `column_band`.
        for &index in legend_band {
            assert!(
                spans[index].text.starts_with("LEG-"),
                "a legend fragment landed outside the legend band: {}",
                spans[index].text
            );
        }
        for &index in column_band {
            assert!(
                !spans[index].text.starts_with("LEG-"),
                "a column line leaked into the legend band: {}",
                spans[index].text
            );
        }
    }

    /// a right-column cell set a fraction of a point ABOVE a left-column body line shares
    /// its `y` bucket and used to sort ahead of it, so the gap between them came out hundreds
    /// of points negative and the two columns were fused into one "full-width" line. Two such
    /// rows were then peeled as a band: the table's last column welded into the left column's
    /// prose (the shape of a two-column journal page with a table in its right column). ~keep
    #[test]
    fn a_right_cell_just_above_a_left_line_is_not_one_full_width_line() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();
        for row in 0..6 {
            let y = 600.0 - row as f32 * 10.0;
            spans.push(make_span_text(38.0, y, 253.0, 8.0, "left body line", 8.0)); // 38..291
            // the right column's last table cell, set 0.3pt higher than the body line
            spans.push(make_span_text(535.0, y + 0.3, 16.0, 6.4, "0.809", 6.4)); // 535..551
            spans.push(make_span_text(307.0, y + 0.3, 60.0, 6.4, "cell", 6.4)); // 307..367
        }
        let indices: Vec<usize> = (0..spans.len()).collect();

        for line in group_indices_into_lines(&spans, &indices) {
            let left = line.iter().any(|&i| spans[i].bbox.left() < 300.0);
            let right = line.iter().any(|&i| spans[i].bbox.left() > 300.0);
            assert!(
                !(left && right),
                "a line may not hold both columns: {:?}",
                line.iter().map(|&i| &spans[i].text).collect::<Vec<_>>()
            );
        }
        assert!(
            strategy.peel_full_width_line_bands(&spans, &indices).is_none(),
            "no line here is full width, so nothing may be peeled"
        );
    }

    /// a running header whose bbox runs 465 pt past its 18 characters is not a
    /// full-width line, and a left-column line it overlaps is not fused with the right
    /// column's text on the same baseline. ~keep
    #[test]
    fn an_inflated_bbox_does_not_make_a_line_full_width() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();
        for row in 0..4 {
            let y = 700.0 - row as f32 * 10.0;
            // left column line whose bbox overreaches into the right column
            spans.push(make_span_text(
                38.0,
                y,
                464.0,
                8.0,
                "J.S. Levine et al.                                        ",
                6.4,
            )); // 38..502
            spans.push(make_span_text(400.0, y, 160.0, 8.0, "Journal of Lorem Ipsum 12", 6.4)); // 400..560
        }
        let indices: Vec<usize> = (0..spans.len()).collect();
        assert!(
            strategy.peel_full_width_line_bands(&spans, &indices).is_none(),
            "an inflated bbox must not make its line full width"
        );
        let (left, right) = strategy.partition_lines_at(&spans, &indices, 300.0);
        assert!(left.iter().all(|&i| spans[i].bbox.left() < 300.0), "{left:?}");
        assert!(right.iter().all(|&i| spans[i].bbox.left() > 300.0), "{right:?}");
    }

    /// a zero-width space in the right column's table has no ink on either side of the
    /// cut; it must stay right, not be sent left by the tie rule into the left column. ~keep
    #[test]
    fn a_zero_width_span_stays_on_its_own_side() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(38.0, 600.0, 253.0, 8.0, "left body line", 8.0),
            make_span_text(530.0, 590.0, 0.0, 8.0, "\u{200b}", 8.0),
            make_span_text(307.0, 600.0, 200.0, 8.0, "right body line", 8.0),
        ];
        let indices: Vec<usize> = (0..spans.len()).collect();
        let (left, right) = strategy.partition_lines_at(&spans, &indices, 299.0);
        assert_eq!(left, vec![0]);
        assert_eq!(right, vec![1, 2]);
    }

    /// a clause with a hanging number is one row of two lines; the peel must move the
    /// number with its full-width text, not strand it in the other band. ~keep
    #[test]
    fn a_hanging_number_goes_with_its_full_width_clause() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();
        for row in 0..3 {
            let y = 700.0 - row as f32 * 12.0;
            spans.push(make_span_text(71.0, y, 22.0, 9.0, "24.1", 9.0));
            spans.push(make_span_text(
                110.0,
                y,
                422.0,
                9.0,
                "Partijen verstrekken elkaar tijdig alle relevante informatie al dan niet afkomstig van derden die",
                9.0,
            ));
        }
        for row in 0..3 {
            let y = 600.0 - row as f32 * 12.0;
            spans.push(make_span_text(
                71.0,
                y,
                200.0,
                9.0,
                "short line of a list item text",
                9.0,
            ));
        }
        let indices: Vec<usize> = (0..spans.len()).collect();
        let bands = strategy
            .peel_full_width_line_bands(&spans, &indices)
            .expect("the clause rows are full width");
        let band_of = |i: usize| bands.iter().position(|b| b.contains(&i)).unwrap();
        for row in 0..3 {
            assert_eq!(
                band_of(2 * row),
                band_of(2 * row + 1),
                "number and text of row {row} split: {bands:?}"
            );
        }
    }

    /// a title that starts a few points left of a cut placed inside a column stays with
    /// its hanging number, on the side it starts. ~keep
    #[test]
    fn a_title_starting_just_before_the_cut_stays_with_its_number() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(48.2, 297.2, 13.4, 9.0, "7.5", 9.0),
            make_span_text(
                79.3,
                297.2,
                170.8,
                9.0,
                "Bedrijfsdruk van de CV-installatie instellen",
                9.0,
            ),
            make_span_text(100.0, 280.0, 150.0, 9.0, "indented list content", 9.0),
        ];
        let indices: Vec<usize> = (0..spans.len()).collect();
        let (left, right) = strategy.partition_lines_at(&spans, &indices, 91.7);
        assert_eq!(left, vec![0, 1]);
        assert_eq!(right, vec![2]);
    }

    /// a hanging number and a title that crosses the cut move together; two column
    /// lines on one baseline, with the cut in the gutter between them, do not. ~keep
    #[test]
    fn a_crossing_title_moves_with_its_hanging_number() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(42.5, 651.2, 25.0, 9.0, "6.4.3", 9.0),
            make_span_text(
                85.0,
                651.2,
                296.3,
                9.0,
                "Minimale grondoppervlakte en oppervlakte van ventilatieopeningen",
                9.0,
            ),
            make_span_text(
                38.0,
                600.0,
                253.0,
                8.0,
                "left column body line of ordinary prose text here",
                8.0,
            ),
            make_span_text(
                307.0,
                600.0,
                253.0,
                8.0,
                "right column body line of ordinary prose text here",
                8.0,
            ),
        ];
        let indices: Vec<usize> = (0..spans.len()).collect();
        let (left, right) = strategy.partition_lines_at(&spans, &indices, 223.5);
        assert!(
            left.contains(&0) && left.contains(&1),
            "number and title must stay together: {left:?} / {right:?}"
        );
        let (left, right) = strategy.partition_lines_at(&spans, &indices, 299.0);
        assert!(
            left.contains(&2) && right.contains(&3),
            "column lines split at the gutter: {left:?} / {right:?}"
        );
    }

    /// two column lines on one row are never one unit, even when the cut runs through
    /// the left one rather than the gutter. ~keep
    #[test]
    fn a_left_column_line_is_not_a_lead_for_the_right_column() {
        let strategy = XYCutStrategy::new();
        let spans = vec![
            make_span_text(
                50.0,
                158.0,
                241.0,
                8.0,
                "Influenza is an acute respiratory viral infection",
                8.0,
            ),
            make_span_text(
                307.0,
                158.0,
                253.0,
                8.0,
                "of illnesses, ranging from mild symptoms to fatal",
                8.0,
            ),
        ];
        let indices: Vec<usize> = (0..spans.len()).collect();
        let (left, right) = strategy.partition_lines_at(&spans, &indices, 120.0);
        assert_eq!(left, vec![0], "{left:?} / {right:?}");
        assert_eq!(right, vec![1]);
    }

    /// Control for the peel: when each legend line is already ONE span (the reporter's own
    /// p2, which passed before the fix), the peel still fires identically -- the fix must
    /// not change behavior for input that was never torn in the first place. ~keep
    #[test]
    fn peel_full_width_line_bands_control_single_span_legend_gh1808() {
        let strategy = XYCutStrategy::new();
        let mut spans = Vec::new();
        for &y in &[300.0, 285.0, 270.0] {
            spans.push(make_span_text(40.0, y, 100.0, 10.0, "left column line", 10.0));
            spans.push(make_span_text(200.0, y, 100.0, 10.0, "right column line", 10.0));
        }
        for row in 0..8 {
            let y = 150.0 - row as f32 * 12.0;
            spans.push(make_span_text(
                40.0,
                y,
                260.0,
                7.17,
                "LEGEND lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod",
                7.17,
            ));
        }
        let indices: Vec<usize> = (0..spans.len()).collect();

        let bands = strategy
            .peel_full_width_line_bands(&spans, &indices)
            .expect("the single-span legend must still be peeled off");
        assert_eq!(bands.len(), 2);
        assert_eq!(
            bands[1].len(),
            8,
            "8 single-span legend lines must form the legend band"
        );
    }

    /// RED-then-GREEN unit test for GH#1808's assignment fix (`partition_lines_at`), in
    /// isolation from the peel. A continuous 5-fragment line (font-run gaps of 4pt at
    /// 20pt font, well under `MAX_INTRA_LINE_GAP_EM`) runs from x=40 to x=420, straddling
    /// `split_x = 200.0` with the majority of its inked width (212 of 394pt) on the right.
    /// Before the fix (a per-span `left edge < split_x` test) this line tore: its first two
    /// fragments (left edge < 200) landed in `left`, the remaining three in `right`. ~keep
    #[test]
    fn partition_lines_at_keeps_a_straddling_line_whole_gh1808() {
        let strategy = XYCutStrategy::new();
        let mut spans = vec![
            make_span_text(40.0, 300.0, 50.0, 10.0, "clear-left-1", 10.0),
            make_span_text(40.0, 285.0, 50.0, 10.0, "clear-left-2", 10.0),
            make_span_text(300.0, 300.0, 50.0, 10.0, "clear-right-1", 10.0),
            make_span_text(300.0, 285.0, 50.0, 10.0, "clear-right-2", 10.0),
        ];
        let straddling_start = spans.len();
        for (left, right) in [
            (40.0, 116.0),
            (120.0, 196.0),
            (200.0, 276.0),
            (280.0, 356.0),
            (360.0, 420.0),
        ] {
            spans.push(make_span_text(left, 200.0, right - left, 20.0, "STRADDLE", 20.0));
        }
        let indices: Vec<usize> = (0..spans.len()).collect();

        let (left, right) = strategy.partition_lines_at(&spans, &indices, 200.0);

        assert_eq!(left, vec![0, 1], "only the two clear-left lines belong on the left");
        let mut expected_right: Vec<usize> = vec![2, 3];
        expected_right.extend(straddling_start..straddling_start + 5);
        expected_right.sort_unstable();
        let mut actual_right = right;
        actual_right.sort_unstable();
        assert_eq!(
            actual_right, expected_right,
            "the straddling line's 5 fragments must all move to its majority side (right), none left behind"
        );
    }

    /// A single-column list above a ruled table. The table's columns open a valley
    /// at x 246 that the long prose lines are too wide to show up in; cut there, the two
    /// longest steps (more ink right of the cut than left) went into the table's column and
    /// were read after the next heading. Every span of an installation manual's
    /// p40, geometry verbatim, letters replaced by lorem ipsum of the same length. ~keep
    #[test]
    fn a_prose_band_above_a_table_is_not_cut_into_columns() {
        let strategy = XYCutStrategy::new();
        #[rustfmt::skip]
        let page: &[(f32, f32, f32, f32, &str)] = &[
            (87.84, 773.76, 13.68, 12.00, "7.2"),
            (101.52, 773.76, 3.34, 12.00, " "),
            (105.84, 773.76, 147.70, 12.00, "Loremipsumdo lor si tametconsec "),
            (45.36, 756.24, 330.03, 9.60, "Te turadipiscingel its edd oeiusmo dt em po rincidi duntutlab oreetdo lo remagnaali qua § 0.6. "),
            (45.36, 742.32, 343.07, 9.60, "Lore mipsumdolo rsitam etcons ect et uradipiscin gelitsedd oeiusm. Od tem porin ci didu nt utl "),
            (45.36, 731.52, 121.23, 9.60, "aboreetdoloremagn aa liqualore: "),
            (45.36, 714.96, 6.45, 9.60, "1."),
            (51.84, 714.96, 2.67, 9.60, " "),
            (63.36, 714.96, 192.72, 9.60, "Mips umdolorsitam et co  nsectet  ur  adipi  scing, eli tse "),
            (256.56, 714.96, 11.16, 11.04, " 3 "),
            (268.32, 714.96, 184.83, 9.60, " ddoeiusmod te mpo rincidi- du ntu tlaboreetdoloremag. "),
            (45.36, 701.76, 6.45, 9.60, "2."),
            (51.84, 701.76, 2.67, 9.60, " "),
            (63.36, 701.76, 97.44, 9.60, "Naal iqu al  +  or  -  emips  "),
            (161.28, 701.76, 11.26, 11.04, " 48"),
            (172.56, 701.76, 2.80, 10.08, " "),
            (175.92, 701.76, 153.39, 9.60, " (umdolorsita) me tc ons ecteturadipiscinge. "),
            (45.36, 689.76, 6.45, 9.60, "3."),
            (51.84, 689.76, 2.67, 9.60, " "),
            (63.36, 689.76, 271.23, 9.60, "Lits edd oe  iusmodt  empor in ci di duntutl aboreetdo lo re mag naaliqu aloremi. "),
            (45.36, 678.00, 6.45, 9.60, "4."),
            (51.84, 678.00, 2.67, 9.60, " "),
            (63.36, 678.00, 372.51, 9.60, "Psum dol or  +  si  -  tamet co nsectetur ad ip is cingelit seddoe (iusmodtem) po rin cididuntutl aboreet. "),
            (45.36, 663.12, 6.45, 9.60, "5."),
            (51.84, 663.12, 2.67, 9.60, " "),
            (63.36, 663.12, 285.84, 9.60, "Dolo, remag naal iqualore mipsumdolorsi tame tconsecte, tu  radip  iscin ge litsed  "),
            (349.68, 663.12, 5.30, 11.04, "D"),
            (354.96, 663.12, 2.19, 9.60, " "),
            (357.60, 663.12, 116.67, 9.60, " oe ius modtemp orincid iduntutlab. "),
            (45.36, 651.84, 187.47, 9.60, "Or eetdoloremagnaa li qu aloremi psumdolorsitam. "),
            (45.36, 637.92, 2.19, 9.60, " "),
            (45.36, 624.00, 41.41, 9.60, "Etconsect"),
            (86.40, 624.00, 2.19, 9.60, " "),
            (45.36, 609.12, 360.03, 9.60, "Etur ad ipi/sci  ngeli ts ed doeiusm odte mpo rin cid idun tutlab or eetdoloremagnaaliqua lo re mipsu. "),
            (87.84, 591.84, 13.68, 12.00, "7.3"),
            (101.52, 591.84, 3.34, 12.00, " "),
            (105.84, 591.84, 56.26, 12.00, "Mdolorsita "),
            (51.60, 574.08, 21.63, 9.60, "Metc- "),
            (50.64, 563.04, 23.31, 9.60, "onsec "),
            (86.16, 563.04, 138.99, 9.60, "Teturadipisc                   Ingelit Sedd "),
            (240.00, 563.04, 77.07, 9.60, " Oe45  Iu55 Sm51 "),
            (327.12, 563.04, 48.99, 9.60, "Odtemporinci "),
            (59.04, 551.76, 6.51, 9.60, "3 "),
            (86.16, 551.76, 44.94, 9.60, "Diduntutlab "),
            (131.04, 551.76, 13.92, 9.60, "[48]"),
            (144.96, 551.76, 2.19, 9.60, " "),
            (249.60, 551.76, 4.83, 9.60, "- "),
            (277.92, 551.76, 4.83, 9.60, "- "),
            (303.60, 551.76, 4.83, 9.60, "- "),
            (327.12, 551.76, 199.88, 9.60, "Oreetdo lor emagnaaliqualoremipsumdo. Lo rsitametcon sect "),
            (327.12, 540.72, 88.59, 9.60, "eturadipi scinge (=48). "),
            (59.04, 529.44, 6.51, 9.60, "4 "),
            (86.16, 529.44, 49.71, 9.60, "Litseddoeiusmod "),
            (248.88, 529.44, 6.51, 9.60, "4 "),
            (277.20, 529.44, 6.51, 9.60, "4 "),
            (302.64, 529.44, 6.51, 9.60, "4 "),
            (327.12, 529.44, 79.47, 9.60, "3=Tempo Rincidi Du "),
            (327.12, 518.40, 100.83, 9.60, "4=Ntutlab Oree Td + olorem "),
            (327.12, 507.60, 70.83, 9.60, "5=Agnaali Qua Lo "),
            (327.12, 496.80, 72.99, 9.60, "6=Remipsu Mdol Or "),
            (59.04, 485.28, 6.51, 9.60, "5 "),
            (86.16, 485.28, 66.75, 9.60, "Si-tame tconsect "),
            (248.88, 485.28, 6.51, 9.60, "3 "),
            (277.20, 485.28, 6.51, 9.60, "3 "),
            (302.64, 485.28, 6.51, 9.60, "3 "),
            (327.12, 485.28, 90.27, 9.60, "3=eturad ipis cingelits "),
            (327.12, 474.48, 83.07, 9.60, "4=eddo eiusmodt empori "),
            (327.12, 463.44, 179.55, 9.60, "5=ncid iduntutl aboree tdo loremag Naaliqualorem "),
            (59.04, 452.16, 6.51, 9.60, "6 "),
            (86.16, 452.16, 83.55, 9.60, "Ipsumdolo Rs itametco "),
            (244.56, 452.16, 15.15, 9.60, "433 "),
            (272.88, 452.16, 15.15, 9.60, "433 "),
            (298.32, 452.16, 15.15, 9.60, "433 "),
            (327.12, 451.20, 223.47, 9.60, "Nsecteturadi piscingeli tseddo eiusmodte m por 433 (=22 + 4i +) "),
            (58.08, 438.72, 6.45, 9.60, "3."),
            (64.56, 438.72, 2.19, 9.60, " "),
            (86.16, 438.72, 140.19, 9.60, "Ncididu ntutlabore etdoloremag naal "),
            (246.48, 438.72, 10.83, 9.60, "13 "),
            (274.80, 438.72, 10.83, 9.60, "13 "),
            (300.48, 438.72, 10.83, 9.60, "13 "),
            (327.12, 438.72, 148.87, 9.60, "Iqualoremips umdolorsit ametco nsectetur a"),
            (475.92, 438.72, 2.19, 9.60, "."),
            (478.08, 438.72, 35.31, 9.60, " dip 433% "),
            (59.04, 427.20, 6.51, 9.60, "7 "),
            (86.16, 427.20, 84.03, 9.60, "Iscingeli ts eddoeius "),
            (246.72, 427.20, 10.83, 9.60, "13 "),
            (275.04, 427.20, 10.83, 9.60, "13 "),
            (300.48, 427.20, 10.83, 9.60, "13 "),
            (327.12, 426.48, 226.11, 9.60, "Modtemporinc ididuntutl aboree tdolorema g naa 433 (=22 + 4l +)  "),
            (59.04, 414.00, 6.51, 9.60, "8 "),
            (86.16, 414.00, 142.83, 9.60, "Iqu.aloremipsumdolorsi tam et consectet "),
            (246.72, 414.00, 10.83, 9.60, "58 "),
            (275.04, 414.00, 10.83, 9.60, "58 "),
            (300.48, 414.00, 10.83, 9.60, "58 "),
            (327.12, 414.00, 93.39, 9.60, "Uradipiscing 43°E lit 58°S "),
            (59.04, 402.48, 6.51, 9.60, "9 "),
            (86.16, 402.48, 136.11, 9.60, "Edd.oeiusmodtemporinc idi du ntutlabor "),
            (247.44, 402.48, 9.15, 9.60, "-0 "),
            (275.76, 402.48, 9.15, 9.60, "-0 "),
            (301.44, 402.48, 9.15, 9.60, "-0 "),
            (327.12, 402.48, 91.71, 9.60, "Eetdoloremag -2°N aal 43°I "),
            (59.04, 391.20, 6.51, 9.60, "0 "),
            (86.16, 391.20, 140.43, 9.60, "Qua. loremipsumdolorsi tam et consectet "),
            (246.72, 391.20, 10.83, 9.60, "58 "),
            (275.04, 391.20, 10.83, 9.60, "58 "),
            (300.48, 391.20, 10.83, 9.60, "58 "),
            (327.12, 391.20, 93.39, 9.60, "Uradipiscing 48°E lit 63°S "),
            (59.04, 379.68, 6.51, 9.60, "1 "),
            (86.16, 379.68, 120.27, 9.60, "Ed-doei usmodtempor in Ci diduntu "),
            (248.88, 379.68, 6.51, 9.60, "4 "),
            (277.20, 379.68, 6.51, 9.60, "4 "),
            (302.64, 379.68, 6.51, 9.60, "4 "),
            (327.12, 379.68, 93.87, 9.60, "Tlaboreetdol 3 - 48 oremagn "),
            (59.04, 368.40, 6.51, 9.60, "2 "),
            (86.16, 368.40, 130.83, 9.60, "Aa-liqu aloremipsum  do lorsit ametcon "),
            (248.88, 368.40, 6.51, 9.60, "4 "),
            (277.20, 368.40, 6.51, 9.60, "4 "),
            (302.64, 368.40, 6.51, 9.60, "4 "),
            (327.12, 368.40, 184.83, 9.60, "Secteturadip 3 - 48 iscinge (l.i.t. sedd Oeius modtemp) "),
            (58.56, 356.88, 7.47, 9.60, "O "),
            (86.16, 356.88, 120.03, 9.60, "Rinci diduntutlab or eetdolore Mag "),
            (248.88, 356.88, 6.51, 9.60, "3 "),
            (277.20, 356.88, 6.51, 9.60, "3 "),
            (302.64, 356.88, 6.51, 9.60, "3 "),
            (327.12, 356.88, 112.35, 9.60, "3=naaliqu Al oremips umdolorsita "),
            (327.12, 346.08, 112.83, 9.60, "4=metcons ec teturad ipiscingeli "),
            (327.12, 335.04, 99.09, 9.60, "5=tseddoeiusm odtemp orincid "),
            (426.24, 335.04, 13.89, 9.60, "idun"),
            (440.16, 335.04, 25.71, 9.60, " tu tlab "),
            (327.12, 324.24, 83.31, 9.60, "0=Or/Ee tdolorem agnaal "),
            (59.04, 312.72, 6.51, 9.60, "i "),
            (86.16, 312.72, 29.07, 9.60, "Qualore "),
            (248.88, 312.72, 6.51, 9.60, "3 "),
            (277.20, 312.72, 6.51, 9.60, "3 "),
            (302.64, 312.72, 6.51, 9.60, "3 "),
            (327.12, 312.72, 21.39, 9.60, "3=mip  "),
            (327.12, 301.92, 139.71, 9.60, "4=sum (dolors ita metconsecteturad 3 ip 5) "),
            (58.56, 290.40, 7.71, 9.60, "I "),
            (86.16, 290.40, 65.07, 9.60, "Scingelitseddoei "),
            (248.88, 290.40, 6.51, 9.60, "4 "),
            (277.20, 290.40, 6.51, 9.60, "4 "),
            (302.64, 290.40, 6.51, 9.60, "4 "),
            (327.12, 290.40, 144.03, 9.60, "3=usmodtemporincid iduntut La boreetd olo "),
            (327.12, 279.60, 148.83, 9.60, "4=remagnaaliqualor emipsum Do lorsita met "),
            (59.28, 268.08, 6.03, 9.60, "c "),
            (86.16, 268.08, 78.75, 9.60, "Onsectet uradipisc In "),
            (246.72, 268.08, 10.83, 9.60, "68 "),
            (275.04, 268.08, 10.83, 9.60, "63 "),
            (300.48, 268.08, 10.83, 9.60, "63 "),
            (327.12, 268.08, 128.91, 9.60, "Gelitseddoei 58 - 73% (usmodte = 73) "),
            (58.32, 256.80, 8.19, 9.60, "m. "),
            (86.16, 256.80, 138.03, 9.60, "Porinci diduntutla boreetdolor emag "),
            (246.48, 256.80, 10.83, 9.60, "73 "),
            (274.80, 256.80, 10.83, 9.60, "73 "),
            (300.48, 256.80, 10.83, 9.60, "73 "),
            (327.12, 256.80, 183.83, 9.60, "Naaliqualore : 3, 48 mip sumdolorsi tametc onsectetu 6"),
            (510.96, 256.80, 4.59, 9.60, ". "),
            (328.80, 245.76, 181.23, 9.60, "R.A.   3 = Dipi scingelits eddo eiusmodtem por Inc "),
            (364.32, 234.96, 106.11, 9.60, "ididuntut laboreetdol orem "),
            (58.56, 223.68, 7.71, 9.60, "A "),
            (86.16, 223.68, 79.23, 9.60, "Gnaaliqu aloremips um "),
            (246.72, 223.68, 10.83, 9.60, "68 "),
            (275.04, 223.68, 10.83, 9.60, "63 "),
            (300.48, 223.68, 10.83, 9.60, "63 "),
            (327.12, 223.68, 128.91, 9.60, "Dolorsitamet 58 - 73% (consect = 73) "),
            (58.56, 212.16, 7.47, 9.60, "E "),
            (86.16, 212.16, 126.67, 9.60, "Tur. adipiscingelitsedd oeiusmo Dt "),
            (246.72, 212.16, 10.83, 9.60, "73 "),
            (275.04, 212.16, 10.83, 9.60, "73 "),
            (300.48, 212.16, 10.83, 9.60, "73 "),
            (327.12, 212.16, 197.31, 9.60, "Emporincidid 43°U -  93°N. Tutlab or Ee tdoloremagn aal "),
            (86.16, 201.36, 138.03, 9.60, "iqual   (Or = Emipsumdo lorsitametc) "),
            (327.12, 201.36, 209.73, 9.60, "onsecte turadipisci ngelit sed doeiu sm odt empo rincididun "),
            (327.12, 190.32, 202.59, 9.60, "tutlab, ore etd ol oremagnaali qual oremipsumd olorsi. "),
            (57.60, 179.04, 9.63, 9.60, "T. "),
            (86.16, 179.04, 38.67, 9.60, "Am etconse "),
            (248.88, 179.04, 6.51, 9.60, "4 "),
            (277.20, 179.04, 6.51, 9.60, "4 "),
            (302.64, 179.04, 6.51, 9.60, "4 "),
            (327.12, 179.04, 91.95, 9.60, "3= Ct eturadi piscin < G "),
            (327.12, 168.00, 112.59, 9.60, "4= El its eddoeiusm odtemp < O "),
            (327.12, 157.20, 49.95, 9.60, "5= Ri nci-did "),
            (58.80, 145.68, 6.99, 9.60, "U "),
            (86.16, 145.68, 61.95, 9.60, "Ntutlaboreetdo Lo "),
            (246.72, 145.68, 10.83, 9.60, "03 "),
            (275.04, 145.68, 10.83, 9.60, "03 "),
            (300.48, 145.68, 10.83, 9.60, "03 "),
            (327.12, 145.68, 210.99, 9.60, "Remagnaaliqu 83 - 22% alo rem ipsumdolor sitametc onsectetu. "),
            (57.84, 134.40, 9.15, 9.60, "R. "),
            (86.16, 134.40, 65.79, 9.60, "Adipiscingelit Se "),
            (246.72, 134.40, 10.83, 9.60, "03 "),
            (275.04, 134.40, 10.83, 9.60, "03 "),
            (300.48, 134.40, 10.83, 9.60, "03 "),
            (327.12, 134.40, 210.99, 9.60, "Ddoeiusmodte 83 - 22% mpo rin cididuntut laboreet doloremag. "),
            (45.36, 122.88, 2.19, 9.60, " "),
            (189.36, 122.88, 2.19, 9.60, " "),
            (45.36, 18.48, 115.98, 10.08, "Naaliqua Loremipsum Do"),
            (161.04, 18.48, 5.44, 10.08, "  "),
            (539.52, 18.48, 9.12, 10.08, "73"),
            (548.64, 18.48, 2.40, 9.60, " "),
        ];
        let spans: Vec<TextSpan> = page
            .iter()
            .map(|&(x, y, w, size, text)| make_span_text(x, y, w, size, text, size))
            .collect();
        let indices: Vec<usize> = (0..spans.len()).collect();
        let order: Vec<usize> = strategy
            .partition_indexed(&spans, &indices)
            .into_iter()
            .flatten()
            .collect();
        let pos = |i: usize| order.iter().position(|&o| o == i).unwrap();
        let find = |text: &str, y: f32| {
            spans
                .iter()
                .position(|s| s.text == text && (s.bbox.y - y).abs() < 0.5)
                .unwrap()
        };
        let next_heading = find("7.3", 591.84);
        let steps: Vec<usize> = [714.96, 701.76, 689.76, 678.0, 663.12]
            .iter()
            .zip(["1.", "2.", "3.", "4.", "5."])
            .map(|(&y, n)| find(n, y))
            .collect();
        for pair in steps.windows(2) {
            assert!(pos(pair[0]) < pos(pair[1]), "steps out of order");
        }
        for (i, s) in spans.iter().enumerate() {
            if (660.0..720.0).contains(&s.bbox.y) {
                assert!(
                    pos(i) < pos(next_heading),
                    "step text {:?} read after the next heading",
                    s.text
                );
            }
        }
    }

    /// A report page -- a text column left, a chart right, and wide quotes under both.
    /// The quotes cross the gutter with nothing beside them: a prose band. Peeled, the two
    /// columns above still read column by column; refusing the cut instead read the region
    /// by y and wove the text column into the chart line by line. Every span of the page,
    /// geometry verbatim, letters replaced by lorem ipsum of the same length. ~keep
    #[test]
    fn two_columns_above_a_band_of_wide_quotes_keep_their_order() {
        let strategy = XYCutStrategy::new();
        #[rustfmt::skip]
        let page: &[(f32, f32, f32, f32, &str)] = &[
            (300.79, 744.84, 12.81, 9.00, "13 "),
            (256.85, 730.68, 100.55, 9.00, "Lor Emipsumd Olorsi "),
            (66.62, 689.14, 200.41, 11.04, "Tame tconsec tet ura dipi scing elit se "),
            (66.62, 673.18, 230.56, 11.04, "ddoeiusmo dte mpor in cididu nt utlabor ee tdolo "),
            (315.07, 671.74, 226.24, 12.96, "Rem-ag-naal Iqua loremip sum dol orsit "),
            (66.62, 657.10, 222.99, 11.04, "ame tcon secte tu radipisci ng elit seddoeiu. "),
            (315.07, 656.62, 215.88, 12.96, "dolo rem agnaaliq ualor emipsum dolo "),
            (315.07, 641.62, 220.97, 12.96, "rsitametc onse cteturadi pi s cingel "),
            (66.62, 641.14, 212.86, 11.04, "Smodtem por-in-cidi du ntut labor (42%) eet "),
            (66.62, 625.18, 230.62, 11.04, "itsed doei usm odte mpo rincidi duntut laboree "),
            (315.07, 625.06, 189.74, 9.00, "Tdolo rem 46% ag naali qu aloremi, psumdolors, "),
            (315.07, 613.06, 198.47, 9.00, "itametconse cte tura dipi sci nge litse ddoe iu "),
            (66.62, 609.10, 208.91, 11.04, "sm odt emporinci diduntutl abor eetdolore. "),
            (315.07, 601.06, 227.84, 9.00, "magnaaliq ual orem ip sumdol ors itam et consect et uradi "),
            (66.62, 593.14, 222.78, 11.04, "Piscingelit sedd oeiusmod te mpo rinci didu "),
            (315.07, 589.06, 197.46, 9.00, "ntu, % tla bor eetd ol ore magnaaliq ual oremips "),
            (66.62, 577.18, 226.42, 11.04, "umdol or sitametco nsecte turadipisci ngeli "),
            (315.07, 568.06, 135.54, 8.04, "Tsed Doeius Modte Mp Orincidid "),
            (450.58, 568.06, 2.25, 9.00, " "),
            (523.08, 568.06, 8.73, 9.00, "% "),
            (66.62, 561.10, 225.14, 11.04, "untut laboreetdo lo remagnaaliqua loremipsumd "),
            (315.07, 555.70, 95.64, 9.00, "Olorsitam et consectet  "),
            (521.04, 555.70, 12.81, 9.00, "42 "),
            (66.62, 545.14, 158.93, 11.04, "urad ipis cingelit seddoeiusmo.  "),
            (315.07, 541.30, 133.20, 9.00, "Dtempori nc ididunt utlaboreetdolo  "),
            (521.04, 541.30, 12.81, 9.00, "42 "),
            (315.07, 526.87, 188.06, 9.00, "Remagna aliq ualorem ipsumd olo rsitam etconsecte  "),
            (521.04, 526.87, 12.81, 9.00, "47 "),
            (315.07, 512.47, 143.16, 9.00, "Turadi, piscingel its eddoei usmodtem  "),
            (521.04, 512.47, 12.81, 9.00, "45 "),
            (102.62, 509.95, 199.46, 11.04, "\"P'o rincididuntut laboree td ol O'r ema "),
            (315.07, 498.07, 146.16, 9.00, "Gnaaliqualo remipsumd olorsitamet  "),
            (523.68, 498.07, 7.53, 9.00, "2 "),
            (102.62, 495.67, 185.17, 11.04, "conse ct et u radi piscin gelitse dd "),
            (315.07, 485.11, 190.72, 9.00, "Oeiu sm odte mporin cidi duntut; labo re etdolorem "),
            (102.62, 481.27, 103.70, 11.04, "agn aaliq ua lo remi.\""),
            (206.33, 481.27, 80.38, 11.04, " - Psumd olors, "),
            (523.68, 480.55, 7.53, 9.00, "2 "),
            (315.07, 476.11, 40.41, 9.00, "itametcon "),
            (102.62, 466.87, 50.54, 11.04, "secte, 65  "),
            (315.07, 461.71, 155.64, 9.00, "Tura dipisci nge lits edd Oeiusm Odtemp "),
            (523.68, 461.71, 7.53, 9.00, "7 "),
            (315.07, 447.31, 74.88, 9.00, "Agn aaliq ualoremip "),
            (521.04, 447.31, 12.81, 9.00, "43 "),
            (102.62, 446.47, 133.14, 11.04, "\"Orinci didu ntutlaboreet "),
            (235.85, 446.47, 40.74, 11.04, "dolor em "),
            (315.07, 432.91, 42.57, 9.00, "Ad ipisci "),
            (523.68, 432.91, 7.53, 9.00, "1 "),
            (102.62, 432.07, 142.29, 11.04, "sumd O lo rsitame tc onsec.\""),
            (244.97, 432.07, 38.79, 11.04, " - Tetur "),
            (102.62, 417.79, 130.73, 11.04, "nge, litseddo eiusmodte, 62 "),
            (315.07, 414.91, 220.48, 8.04, "Mpor: Incid id Untu tlabore etd olo remag naal iq ualoremip sum "),
            (315.07, 404.95, 225.62, 8.04, "dolo rs itamet con sect et uradipi sc ingel its (e=549). Ddoe-ius "),
            (102.62, 397.39, 182.13, 11.04, "\"Modtempor inc ididun tutl abor ee "),
            (315.07, 394.99, 213.35, 8.04, "tdolorema gna aliqu alor emipsumdol. Orsitam etc on sect etur "),
            (315.07, 384.91, 166.68, 8.04, "433% mporinc ididuntu tlaboreet dolo remagna. "),
            (102.62, 382.99, 38.18, 11.04, "adipis.\""),
            (140.78, 382.99, 149.01, 11.04, " - Cingelit seddo, eiusmodte, "),
            (315.07, 374.95, 215.18, 8.04, "Aliqua: Loremi ps U.M. dolors itametcon Sect 44-Etu. 43, 5340. "),
            (102.62, 368.57, 18.14, 11.04, "71  "),
            (315.07, 364.97, 219.66, 8.04, "\"Radip isc Ing el Itse Ddoei us Modt Empo Rincididu Ntutla\""),
            (534.72, 364.97, 2.01, 8.04, " "),
            (315.07, 350.57, 87.60, 8.04, "Bor Eetdolor Emagna "),
            (102.62, 348.29, 196.56, 11.04, "\"Aliqua lore mi psu mdolo rs it amet con "),
            (102.62, 333.89, 198.58, 11.04, "secteturadipi scing elitsedd oe iusmodt "),
            (102.62, 319.49, 149.39, 11.04, "emp or incididun tu tl abo.\""),
            (252.05, 319.49, 140.23, 11.04, " - Reetd olore, magnaal, 78 "),
            (66.62, 299.09, 450.20, 11.04, "Iqualor 47% emip s umdolor sita metcons ectetu rad ipisci ngelitsedd oe ius modtempor inc idi "),
            (66.62, 284.69, 34.10, 11.04, "duntu: "),
            (102.62, 264.41, 287.58, 11.04, "\"Tl aboree tdo loremagna al iq ualo. Re mips um dolo rsita.\""),
            (390.34, 264.41, 126.13, 11.04, " - Metco nsect, eturadip "),
            (102.62, 250.01, 88.61, 11.04, "iscingelitsed, 93 "),
            (102.62, 232.73, 324.64, 11.04, "\"Doe iusmodtem po rinci didunt ut lab oreetdolo re magnaa liqu alo"),
            (427.42, 232.73, 118.65, 11.04, "remipsu mdolorsita me tco "),
            (102.62, 216.77, 257.57, 11.04, "nsec te turad ip iscinge litseddo ei usm odte mpori.\""),
            (360.19, 216.77, 145.51, 11.04, " - Ncididun tut, laboreet, 98 "),
            (66.62, 184.70, 472.29, 11.04, "Dolor ema-gn-aaliq (45%) ua lore mipsu mdo lors itam etcon sectetur adipisci ng elitse, ddoeiusmod "),
            (66.62, 168.74, 433.78, 11.04, "tem porinc id idu ntut la boree tdo lo remag naaliqualorem ips umdolorsi tame tcons ectet "),
            (66.62, 152.66, 55.58, 11.04, "uradipisc. "),
            (102.62, 123.62, 395.34, 11.04, "\"In g elits eddoe I usm odtemp orin cid iduntutlab or eetdolorema gnaaliq ua "),
            (102.62, 109.22, 433.64, 11.04, "loremipsumd. Ol or sitametc onse ctetu radipi sc i ngeli tse ddoe ius modtemporin cidid un "),
            (264.41, 37.82, 85.55, 9.00, "tut.laboreetdol.ore "),
        ];
        let spans: Vec<TextSpan> = page
            .iter()
            .map(|&(x, y, w, size, text)| make_span_text(x, y, w, size, text, size))
            .collect();
        let indices: Vec<usize> = (0..spans.len()).collect();
        let order: Vec<usize> = strategy
            .partition_indexed(&spans, &indices)
            .into_iter()
            .flatten()
            .collect();
        let pos = |i: usize| order.iter().position(|&o| o == i).unwrap();
        let text_column: Vec<usize> = (0..spans.len())
            .filter(|&i| spans[i].bbox.right() < 305.0 && (540.0..700.0).contains(&spans[i].bbox.y))
            .collect();
        let chart: Vec<usize> = (0..spans.len())
            .filter(|&i| spans[i].bbox.left() >= 315.0 && (350.0..690.0).contains(&spans[i].bbox.y))
            .collect();
        assert!(text_column.len() >= 8 && chart.len() >= 8);
        let last_text = text_column.iter().map(|&i| pos(i)).max().unwrap();
        let first_chart = chart.iter().map(|&i| pos(i)).min().unwrap();
        assert!(
            last_text < first_chart,
            "the text column is woven into the chart: {order:?}"
        );
    }

    /// A two-column journal page, a sparse table in the left column (Soluble p6). The
    /// widest interior valley is the right column's shoulder at x ~ 520-545 -- the 0.45 em ink
    /// estimate ends its lines early, and header numbers stand past it -- and its cut is too
    /// narrow on the right; the real gutter (~16 pt) is the next valley. With only the widest
    /// tried the page got no column cut and was read by y, the table note line by line into the
    /// right column's prose. Every span of the page, geometry verbatim, letters replaced by lorem
    /// ipsum of the same length. ~keep
    #[test]
    fn a_column_cut_falls_back_to_the_next_valley() {
        use crate::layout::FontWeight;
        let strategy = XYCutStrategy::new();
        #[rustfmt::skip]
        let page: &[(f32, f32, f32, f32, bool, &str)] = &[
            (37.59, 752.43, 463.75, 6.38, false, "R. Eetdolore ma gn.                                                                                                                                                                                                                               "),
            (400.27, 752.42, 20.35, 6.38, false, "Dipisci"),
            (420.62, 752.42, 1.87, 6.38, false, " "),
            (422.48, 752.42, 5.28, 6.38, false, "ng"),
            (427.76, 752.42, 1.87, 6.38, false, " "),
            (429.62, 752.42, 35.42, 6.38, false, "Elitseddoeius"),
            (465.04, 752.42, 1.87, 6.38, false, " "),
            (466.91, 752.42, 37.90, 6.38, false, "Modtemporinc"),
            (504.81, 752.42, 1.87, 6.38, false, " "),
            (506.68, 752.42, 7.09, 6.38, false, "45"),
            (513.77, 752.42, 1.87, 6.38, false, " "),
            (515.64, 752.42, 19.52, 6.38, false, "(5359)"),
            (535.15, 752.42, 1.87, 6.38, false, " "),
            (537.02, 752.42, 21.27, 6.38, false, "433694"),
            (558.29, 752.42, 1.87, 6.38, false, " "),
            (37.59, 732.59, 27.29, 7.17, true, "Lorem 5 "),
            (37.59, 723.00, 195.27, 7.17, false, "Psumdolo rsitametco nsecteturadipis ci Nge litseddo: Eiusmod (t "),
            (232.78, 723.00, 5.52, 7.17, false, "~"),
            (240.32, 723.00, 50.45, 7.17, false, "68), Emporincidi "),
            (37.59, 713.42, 43.84, 7.17, false, "Ipsumdolo (r "),
            (81.52, 713.42, 5.52, 7.17, false, "~"),
            (89.18, 713.42, 93.34, 7.17, false, "47), sit Ametcons ec Tetu (r "),
            (182.55, 713.42, 5.52, 7.17, false, "~"),
            (190.26, 713.42, 12.78, 7.17, false, "54)."),
            (43.60, 700.33, 32.75, 6.38, false, "Adipiscing "),
            (96.72, 700.33, 16.99, 6.38, false, "Aal i "),
            (113.72, 700.33, 4.91, 6.38, false, "~"),
            (120.58, 700.33, 7.18, 6.38, false, "68"),
            (157.78, 700.33, 31.70, 6.38, false, "Duntutlabor "),
            (202.84, 700.33, 11.48, 6.38, false, "Agn "),
            (260.90, 700.33, 20.87, 6.38, false, "a-aliqu"),
            (43.60, 691.71, 40.45, 6.38, false, "elitseddoeiusmo"),
            (157.78, 691.71, 34.91, 6.38, false, "eetdolore m "),
            (202.84, 691.71, 46.41, 6.38, false, "Aloremip sumd o "),
            (157.78, 683.15, 4.91, 6.38, false, "~"),
            (164.63, 683.15, 7.18, 6.38, false, "47"),
            (202.84, 683.15, 4.91, 6.38, false, "~"),
            (209.65, 683.15, 7.18, 6.38, false, "54"),
            (43.60, 670.00, 32.42, 6.38, true, "Dte, mp/O"),
            (116.45, 670.00, 13.88, 6.38, false, "5 (6)"),
            (157.78, 670.00, 17.47, 6.38, false, "7 (48)"),
            (218.95, 670.00, 13.94, 6.38, false, "5 (6)"),
            (265.83, 670.00, 16.14, 6.38, false, "3.437"),
            (43.60, 661.44, 38.17, 6.38, true, "rin58,C/id"),
            (109.25, 661.44, 28.26, 6.38, false, "787 (752)"),
            (157.78, 661.44, 28.26, 6.38, false, "076 (026)"),
            (211.80, 661.44, 28.26, 6.38, false, "693 (549)"),
            (260.90, 661.44, 4.78, 6.38, true, "<"),
            (265.68, 661.44, 17.00, 6.38, true, "3.334"),
            (43.60, 652.88, 35.10, 6.38, true, "id-untut, "),
            (109.25, 652.88, 28.26, 6.38, false, "899 (613)"),
            (157.78, 652.88, 28.26, 6.38, false, "820 (760)"),
            (211.80, 652.88, 28.26, 6.38, false, "832 (608)"),
            (265.83, 652.88, 16.14, 6.38, false, "3.759"),
            (49.55, 644.32, 22.20, 6.38, true, "labo/R"),
            (43.60, 635.75, 39.53, 6.38, true, "Eetd, ol/or"),
            (107.43, 635.75, 28.27, 6.38, false, "4.03 (4.4)"),
            (157.78, 635.75, 28.22, 6.38, false, "5.68 (6.0)"),
            (209.99, 635.75, 28.27, 6.38, false, "4.93 (4.3)"),
            (265.72, 635.75, 17.00, 6.38, true, "3.365"),
            (43.60, 627.19, 36.90, 6.38, true, "emag, na/al"),
            (49.95, 618.63, 17.90, 6.38, false, "iqual"),
            (96.72, 618.63, 49.76, 6.38, false, "4518.93 (4483.5)"),
            (157.78, 618.63, 21.60, 6.38, false, "4848.9 "),
            (206.42, 618.63, 39.02, 6.38, false, "216.2 (274.1)"),
            (265.72, 618.63, 17.00, 6.38, true, "3.330"),
            (157.78, 610.02, 24.53, 6.38, false, "(4908.0)"),
            (49.95, 601.46, 18.15, 6.38, false, "ore50"),
            (96.72, 601.46, 49.76, 6.38, false, "6987.63 (5216.4)"),
            (157.78, 601.46, 21.60, 6.38, false, "7764.2 "),
            (202.84, 601.46, 46.18, 6.38, false, "6433.3 (5097.4)"),
            (265.72, 601.46, 17.00, 6.38, true, "3.371"),
            (157.78, 592.90, 24.53, 6.38, false, "(9054.0)"),
            (49.95, 584.34, 18.15, 6.38, false, "mip13"),
            (100.29, 584.34, 39.01, 6.38, false, "401.73 (20.8)"),
            (157.78, 584.34, 18.01, 6.38, false, "420.2 "),
            (206.42, 584.34, 39.02, 6.38, false, "476.6 (431.5)"),
            (265.83, 584.34, 16.14, 6.38, false, "3.568"),
            (157.78, 575.77, 20.94, 6.38, false, "(511.5)"),
            (49.95, 567.21, 21.74, 6.38, false, "sum460"),
            (100.29, 567.21, 39.01, 6.38, false, "477.91 (23.0)"),
            (157.78, 567.21, 18.01, 6.38, false, "417.1 "),
            (206.42, 567.21, 35.43, 6.38, false, "432.7 (06.4)"),
            (265.72, 567.21, 17.00, 6.38, true, "3.335"),
            (157.78, 558.65, 20.94, 6.38, false, "(458.4)"),
            (49.95, 550.04, 21.74, 6.38, false, "dol485"),
            (107.43, 550.04, 31.86, 6.38, false, "77.5 (59.2)"),
            (157.78, 550.04, 31.81, 6.38, false, "84.1 (58.6)"),
            (209.99, 550.04, 31.86, 6.38, false, "74.4 (64.9)"),
            (265.83, 550.04, 16.14, 6.38, false, "3.596"),
            (49.95, 541.48, 17.03, 6.38, false, "orsit"),
            (107.43, 541.48, 31.86, 6.38, false, "98.6 (74.5)"),
            (157.78, 541.48, 31.81, 6.38, false, "00.9 (77.9)"),
            (209.99, 541.48, 31.86, 6.38, false, "87.6 (79.1)"),
            (265.72, 541.48, 17.00, 6.38, true, "3.377"),
            (49.95, 532.91, 13.74, 6.38, false, "amet"),
            (103.86, 532.91, 35.43, 6.38, false, "17.43 (83.0)"),
            (157.78, 532.91, 31.81, 6.38, false, "29.7 (82.2)"),
            (209.99, 532.91, 31.86, 6.38, false, "91.8 (85.0)"),
            (265.72, 532.91, 17.00, 6.38, true, "3.353"),
            (49.95, 524.36, 16.11, 6.38, false, "cons6"),
            (103.86, 524.36, 35.43, 6.38, false, "10.32 (08.1)"),
            (157.78, 524.36, 18.01, 6.38, false, "446.2 "),
            (209.99, 524.36, 31.86, 6.38, false, "92.4 (04.5)"),
            (265.83, 524.36, 16.14, 6.38, false, "3.501"),
            (157.78, 515.79, 17.35, 6.38, false, "(14.7)"),
            (49.95, 507.23, 16.30, 6.38, false, "ect-4"),
            (100.29, 507.23, 39.01, 6.38, false, "436.23 (02.7)"),
            (157.78, 507.23, 18.01, 6.38, false, "441.9 "),
            (209.99, 507.23, 31.86, 6.38, false, "18.5 (17.0)"),
            (265.72, 507.23, 17.00, 6.38, true, "3.371"),
            (157.78, 498.67, 17.35, 6.38, false, "(05.7)"),
            (49.95, 490.11, 19.68, 6.38, false, "etu-R5"),
            (96.72, 490.11, 49.76, 6.38, false, "5751.78 (4951.0)"),
            (157.78, 490.11, 21.60, 6.38, false, "5624.4 "),
            (202.84, 490.11, 46.18, 6.38, false, "5751.7 (5326.6)"),
            (265.83, 490.11, 16.14, 6.38, false, "3.152"),
            (157.78, 481.50, 24.53, 6.38, false, "(4642.2)"),
            (49.95, 472.93, 19.34, 6.38, false, "adip-6"),
            (96.72, 472.93, 49.76, 6.38, false, "5686.63 (4931.4)"),
            (157.78, 472.93, 21.60, 6.38, false, "5007.2 "),
            (202.84, 472.93, 46.18, 6.38, false, "5486.4 (4746.2)"),
            (265.72, 472.93, 17.00, 6.38, true, "3.341"),
            (306.60, 466.81, 22.48, 7.17, true, "Idi. 6."),
            (332.67, 466.81, 227.08, 7.17, false, "D-untutl abor eetd olorem agnaal-iq ua loremips umdo lor sitametcons "),
            (157.78, 464.37, 24.53, 6.38, false, "(4716.2)"),
            (306.60, 457.23, 248.74, 7.17, false, "ecteturad ipisc ing elitsed doeius modtempori ncididun (tutla) bore etdolor~"),
            (43.60, 455.81, 53.68, 6.38, true, "Iscingeli, ts/ed"),
            (306.60, 447.65, 135.52, 7.17, false, "emag naal iq ual oremipsu mdol orsi (t "),
            (443.68, 447.65, 5.52, 7.17, false, "~"),
            (452.86, 447.65, 106.91, 7.17, false, "2). Amet: C- ons E-cteturadip "),
            (49.95, 447.25, 12.62, 6.38, false, "Do4e"),
            (103.86, 447.25, 35.43, 6.38, false, "3.339 (3.07)"),
            (157.78, 447.25, 18.01, 6.38, false, "3.334 "),
            (209.99, 447.25, 31.86, 6.38, false, "3.42 (3.00)"),
            (265.83, 447.25, 16.14, 6.38, false, "3.414"),
            (157.78, 438.69, 17.35, 6.38, false, "(3.83)"),
            (306.60, 438.12, 253.18, 7.17, false, "iscingelit; Se: ddoeius mo dtemporincididu; Ntu: tlaboreetdo 5,6-loremagnaal; "),
            (49.95, 430.13, 9.08, 6.38, false, "Iu9"),
            (107.43, 430.13, 31.86, 6.38, false, "0.54 (40.0)"),
            (157.78, 430.13, 31.81, 6.38, false, "9.14 (40.4)"),
            (209.99, 430.13, 31.86, 6.38, false, "0.94 (56.0)"),
            (265.83, 430.13, 16.14, 6.38, false, "3.809"),
            (306.60, 428.54, 249.20, 7.17, false, "Iq-4: ualoremips umdo lorsi tametco 4; Nse-6: cteturadip iscingelit sedd 6."),
            (49.95, 421.57, 9.08, 6.38, false, "Sm1"),
            (107.43, 421.57, 31.86, 6.38, false, "41.1 (45.9)"),
            (157.78, 421.57, 31.81, 6.38, false, "55.5 (40.8)"),
            (209.99, 421.57, 28.27, 6.38, false, "49.2 (2.9)"),
            (265.83, 421.57, 16.14, 6.38, false, "3.492"),
            (49.95, 412.95, 12.67, 6.38, false, "Od43"),
            (107.43, 412.95, 28.27, 6.38, false, "5.32 (5.6)"),
            (157.78, 412.95, 28.22, 6.38, false, "4.91 (5.5)"),
            (209.99, 412.95, 28.27, 6.38, false, "5.40 (5.6)"),
            (265.83, 412.95, 16.14, 6.38, false, "3.957"),
            (306.59, 406.09, 253.37, 7.97, false, "oeiusm odte mpo rincidi duntutlab oreetdol, oremagnaal i qual ore M "),
            (49.95, 404.39, 12.67, 6.38, false, "Te40"),
            (107.43, 404.39, 28.27, 6.38, false, "7.75 (1.8)"),
            (157.78, 404.39, 31.81, 6.38, false, "6.93 (45.5)"),
            (209.99, 404.39, 28.27, 6.38, false, "7.83 (9.8)"),
            (265.83, 404.39, 16.14, 6.38, false, "3.130"),
            (49.95, 395.83, 15.37, 6.38, false, "Mpor"),
            (103.86, 395.83, 35.43, 6.38, false, "3.334 (3.07)"),
            (157.78, 395.83, 33.04, 6.38, false, "3.334 (3.60"),
            (206.42, 395.83, 35.43, 6.38, false, "3.330 (3.00)"),
            (265.83, 395.83, 16.14, 6.38, false, "3.471"),
            (306.59, 395.66, 253.38, 7.97, false, "ipsu mdolorsitamet co Nse cteturadipis, cingelitsed do ei u smodtem porinc "),
            (49.95, 387.27, 19.01, 6.38, false, "Inci2"),
            (103.86, 387.27, 39.02, 6.38, false, "426.1 (579.2)"),
            (157.78, 387.27, 18.01, 6.38, false, "592.1 "),
            (206.42, 387.27, 39.02, 6.38, false, "492.2 (490.7)"),
            (265.72, 387.27, 17.00, 6.38, true, "3.332"),
            (306.59, 385.17, 170.70, 7.97, false, "id Idu-ntutlab oreetdolorem agn aaliqu alorem."),
            (157.78, 378.71, 20.94, 6.38, false, "(755.1)"),
            (318.55, 374.74, 241.46, 7.97, false, "Ips umdolorsitam etconsec tetura dipi scin geli tseddo eiusmodt "),
            (49.95, 370.15, 22.60, 6.38, false, "Didu43"),
            (103.86, 370.15, 39.02, 6.38, false, "752.6 (857.1)"),
            (157.78, 370.15, 18.01, 6.38, false, "166.5 "),
            (206.42, 370.15, 39.02, 6.38, false, "690.7 (478.5)"),
            (265.72, 370.15, 17.00, 6.38, true, "3.365"),
            (306.59, 364.25, 253.40, 7.97, false, "emporincididu ntutla 9 boreet do loremagnaal iqualoremipsumdol ors "),
            (157.78, 361.59, 20.94, 6.38, false, "(056.0)"),
            (306.59, 353.82, 253.38, 7.97, false, "itametco nse ct etu radi, piscingeli tseddoeiu smodtempori nc ididun "),
            (49.95, 353.03, 22.60, 6.38, false, "Ntut46"),
            (107.43, 353.03, 31.86, 6.38, false, "83.5 (62.6)"),
            (157.78, 353.03, 31.81, 6.38, false, "87.1 (50.7)"),
            (209.99, 353.03, 31.86, 6.38, false, "76.9 (75.0)"),
            (265.83, 353.03, 16.14, 6.38, false, "3.677"),
            (49.95, 344.41, 15.84, 6.38, false, "Labo"),
            (107.43, 344.41, 31.86, 6.38, false, "60.3 (61.8)"),
            (157.78, 344.41, 31.81, 6.38, false, "87.4 (66.4)"),
            (209.99, 344.41, 31.86, 6.38, false, "51.5 (52.1)"),
            (265.72, 344.41, 17.00, 6.38, true, "3.350"),
            (306.59, 343.33, 253.42, 7.97, false, "tutl aboreetdol. Or emagnaal, iqualorem ipsumdol orsita, metconsect etur "),
            (306.59, 332.90, 248.53, 7.97, false, "adipi sci ng elit seddoeius modtempori nc ididuntut laboreet. Dolor~"),
            (37.59, 330.29, 253.21, 7.17, false, "Lorsita metc Ons ect eturadipi. Scin: G elit seddoeiusm odtemp; Orin: C- idi D- "),
            (306.59, 322.41, 253.39, 7.97, false, "emagna, aliqu alor emipsumd 9 olo 45 rsitam etcon secteturad ip isc "),
            (37.59, 320.71, 253.17, 7.17, false, "untutlabor eetdolorem; Ag: naaliqu al oremipsumdolors; Ita: m-etconsec teturad; "),
            (306.59, 311.98, 253.38, 7.97, false, "ingel-itse Ddoe iusmo. Dte mporincidid un tutlabo reet dolore ma gnaal "),
            (37.59, 311.13, 253.19, 7.17, false, "ip-iscin: gelitseddoeiusmo-dtemporincidid Untutl Abo Reetdol; Orem: "),
            (37.59, 301.61, 248.78, 7.17, false, "agnaaliqualore-mipsumd Olor-sitamet consect; Etur: adipiscinge litse ddoe~"),
            (306.59, 301.49, 253.40, 7.97, false, "iqual orem ipsumd, ol ors itametc on secteturadi piscing el itseddoe "),
            (37.59, 292.03, 248.81, 7.17, false, "iusm; Odt: emporincididu ntutl; abor: eetdolo remagn aaliqualor; Emi: psumdo~"),
            (306.59, 291.06, 253.40, 7.97, false, "iusmodtemporin, cididu ntutlab o reetdolor emagnaali-qualore mipsumd "),
            (37.59, 282.44, 21.61, 7.17, false, "lorsi "),
            (64.46, 282.44, 55.44, 7.17, false, "5,6-tametconsec; "),
            (125.18, 282.44, 21.67, 7.17, false, "Tet-6: "),
            (152.05, 282.44, 39.26, 7.17, false, "uradipisci "),
            (196.61, 282.44, 33.73, 7.17, false, "ngelitsedd "),
            (235.56, 282.44, 16.96, 7.17, false, "oeiu "),
            (257.78, 282.44, 8.15, 7.17, false, "6; "),
            (271.16, 282.44, 19.59, 7.17, false, "Sm-4: "),
            (306.59, 280.57, 248.49, 7.97, false, "olorsi. Tametco, nsec teturad ipiscing e lits eddoeiu smodtempori nc id~"),
            (37.59, 272.92, 253.20, 7.17, false, "odtemporin cidi duntu tlabore 4; Et-D5: oloremagna aliq ualor-emipsu 5; Mdo- "),
            (306.59, 270.14, 253.38, 7.97, false, "idun tutlabo, reetdolore magn aali qualorem ipsumdolo rsitame tconsec- "),
            (37.59, 263.34, 253.21, 7.17, false, "6: L-orsi tametconsectet ura dipis cingel 6; its58: eddoeiu smodtempori 5 "),
            (306.59, 259.65, 253.37, 7.97, false, "teturad ipiscingelits eddoeiusm odtemp orin cididu ntutlabore etdolor. "),
            (37.59, 253.76, 253.18, 7.17, false, "ncididun; Tutl: aboreetd oloremagnaa liqual oremip. Sumdolors itamet con Sec "),
            (306.59, 249.22, 253.39, 7.97, false, "Emagn aaliqua loremip sum dolo rs itame tc onsec teturadipi, scingelits ed "),
            (37.59, 244.23, 5.52, 7.17, false, "<"),
            (43.11, 244.23, 54.13, 7.17, false, "43 qu/A, lor58 "),
            (97.28, 244.23, 5.52, 7.17, false, "<"),
            (104.99, 244.23, 34.77, 7.17, false, "933 E/mi."),
            (306.59, 238.73, 253.38, 7.97, false, "doeius Mod. Tempo ri ncididunt utl abor eetdolore magnaaliq ual oremip "),
            (306.59, 228.30, 248.52, 7.97, false, "sumdolorsitame tc Ons, ecteturadi pisc ingelitse Ddo eiusmodte, mporinc~"),
            (37.59, 221.78, 253.42, 7.97, false, "tetura dip iscingelit seddoei us Mod, tem porinc idid untutlabor "),
            (306.59, 217.81, 186.26, 7.97, false, "idid un Tutla bor Eetdo, lor emagnaa liqualo remipsu ["),
            (492.85, 217.81, 4.49, 7.97, false, "7"),
            (497.34, 217.81, 2.23, 7.97, false, ","),
            (499.57, 217.81, 8.97, 7.97, false, "69"),
            (508.55, 217.81, 2.23, 7.97, false, ","),
            (510.78, 217.81, 8.97, 7.97, false, "60"),
            (519.75, 217.81, 40.23, 7.97, false, "], mdolors "),
            (37.59, 211.29, 253.41, 7.97, false, "eetdolor em Agnaa liq Ual-6 oremip su mdolors itametco nsectetura, "),
            (306.59, 207.33, 253.38, 7.97, false, "ita metco nsect eturadi pis. Cin gelitsed doeiu smodte mpor Inc idid "),
            (37.59, 200.86, 81.24, 7.97, false, "dipiscinge litseddoeius "),
            (118.26, 200.86, 49.41, 7.97, false, "modtempori nc "),
            (167.13, 200.86, 6.39, 7.97, false, "i "),
            (172.97, 200.86, 33.02, 7.97, false, "didunt ut "),
            (205.45, 200.86, 30.28, 7.97, false, "laboreet "),
            (235.16, 200.86, 3.37, 7.97, false, "["),
            (238.53, 200.86, 8.97, 7.97, false, "65"),
            (247.50, 200.86, 43.51, 7.97, false, "]. Dolorema, "),
            (306.59, 196.90, 253.38, 7.97, false, "untu-tlab oreetdoloremagnaa liqual oremipsumd olorsita metco nsec "),
            (37.59, 190.38, 248.52, 7.97, false, "gnaal iqua loremip s umdolor sita met consect Etu radipisci ng Eli tsed~"),
            (306.59, 186.41, 49.29, 7.97, false, "tetura dipisc ["),
            (355.88, 186.41, 4.49, 7.97, false, "8"),
            (360.37, 186.41, 199.64, 7.97, false, "]. In gel itseddo eiusm, odtemporinc ididunt utl ab oreetdo "),
            (37.59, 179.94, 253.41, 7.97, false, "doeiusmo dte mporincid idunt ut laboreetd oloremagna al iqualor emipsumd "),
            (306.59, 175.98, 248.51, 7.97, false, "loremagn aa liq ualo-remi psumd olorsita me tconsect etur adipi, sci~"),
            (37.59, 169.46, 85.08, 7.97, false, "olo rsitametc onsectet."),
            (306.59, 165.49, 85.80, 7.97, false, "ngelits edd oeiusmodtempo."),
            (49.55, 159.03, 241.46, 7.97, false, "Ur adipisci ng E lits eddoeiusmod, T empo rincididuntut lab o reetdolo "),
            (318.55, 155.06, 241.47, 7.97, false, "Rincidid untutlabore et d olorema gnaaliq ua Lor, emi psumdolor "),
            (37.59, 148.54, 74.12, 7.97, false, "re Mag naaliqualore ["),
            (111.71, 148.54, 8.97, 7.97, false, "66"),
            (120.68, 148.54, 170.32, 7.97, false, "]. Mipsumd O-lors itametconse, cteturadi piscing "),
            (306.59, 144.57, 31.57, 7.97, false, "sitametc "),
            (337.55, 144.57, 42.35, 7.97, false, "onsectetur "),
            (379.27, 144.57, 7.92, 7.97, false, "ad "),
            (386.58, 144.57, 26.84, 7.97, false, "ipiscing "),
            (412.83, 144.57, 9.27, 7.97, false, "el "),
            (421.51, 144.57, 13.53, 7.97, false, "its "),
            (434.38, 144.57, 47.92, 7.97, false, "eddoeiusmod "),
            (481.71, 144.57, 9.27, 7.97, false, "te "),
            (490.39, 144.57, 14.74, 7.97, false, "Mpo "),
            (504.50, 144.57, 15.41, 7.97, false, "rin "),
            (519.30, 144.57, 40.72, 7.97, false, "cididuntu "),
            (37.59, 138.11, 253.42, 7.97, false, "elitse D doei usmodte mpo rincididu nt utlab O reetd, ol oremag na aliq "),
            (306.59, 134.14, 248.54, 7.97, false, "tlaboreetdol. Oremagn aaliqual oremipsumdol or sit ametconsectet uradi~"),
            (37.59, 127.62, 89.09, 7.97, false, "ua loremips Umdo lorsit ["),
            (126.68, 127.62, 8.97, 7.97, false, "67"),
            (135.65, 127.62, 2.23, 7.97, false, ","),
            (137.88, 127.62, 8.97, 7.97, false, "68"),
            (146.86, 127.62, 144.17, 7.97, false, "]. Ame Tcon/Sect-E tura dipis c ingelits "),
            (306.59, 123.65, 253.40, 7.97, false, "pisc, inge li tseddoeiusm odte (Mp) orincididu ntu Tlabor eetd olor "),
            (37.59, 117.19, 253.41, 7.97, false, "eddo ei U smod temporinci, diduntut, lab oreetdolor. Emagna Aliq ual "),
            (306.59, 113.22, 37.22, 7.97, false, "emagnaal ["),
            (343.81, 113.22, 8.97, 7.97, false, "61"),
            (352.78, 113.22, 207.24, 7.97, false, "]. Iqu alo remi psumd ol orsitame Tconse cte turadipi "),
            (37.59, 106.70, 253.39, 7.97, false, "oremip sumdolorsita M etcon sect eturadip isc ingelit sed doeiusmo dt "),
            (306.59, 102.73, 68.40, 7.97, false, "scingeli tseddoeiu ["),
            (374.99, 102.73, 4.49, 7.97, false, "1"),
            (379.48, 102.73, 180.54, 7.97, false, "]. Smo dtempori ncid, id-untut, laboreet doloremagn "),
            (37.59, 96.27, 248.54, 7.97, false, "emporincidid untutl abore. Et dolor, Emag naaliqua Lor emi Ps-9 umd~"),
            (306.59, 92.30, 253.38, 7.97, false, "aaliqua L oremipsumd olo rsitamet Con secteturad, ipiscin gelitse "),
            (37.59, 85.78, 253.42, 7.97, false, "olorsit, ametconse ctet uradipiscing elitseddoe ius mod-temporin "),
            (306.59, 81.81, 253.41, 7.97, false, "ddoeius M odtemporin ci diduntut lab oreetdol oremagnaa li Qua. Lor "),
            (37.59, 75.35, 37.78, 7.97, false, "cididuntu ["),
            (75.37, 75.35, 8.97, 7.97, false, "66"),
            (84.34, 75.35, 2.23, 7.97, false, ","),
            (86.58, 75.35, 8.97, 7.97, false, "67"),
            (95.55, 75.35, 195.44, 7.97, false, "]. Tlaboree tdolore magn aaliqual oremipsu Mdol orsita "),
            (306.59, 71.38, 253.43, 7.97, false, "emipsumd olor sitametcon se-ctetu ra d ipiscinge li tseddoeiu sm Odt "),
            (37.59, 64.86, 173.45, 7.97, false, "me Tco, nsecteturadi pi scingeli tsed doei usmodtem ["),
            (211.04, 64.86, 8.97, 7.97, false, "68"),
            (220.01, 64.86, 71.02, 7.97, false, "]. Po rinc idid untut "),
            (306.59, 60.89, 15.41, 7.97, false, "emp "),
            (321.39, 60.89, 6.39, 7.97, false, "o "),
            (327.17, 60.89, 33.81, 7.97, false, "rincididu "),
            (360.39, 60.89, 38.73, 7.97, false, "ntutlabor "),
            (398.55, 60.89, 12.39, 7.97, false, "eet "),
            (410.34, 60.89, 14.74, 7.97, false, "Dol "),
            (424.46, 60.89, 47.92, 7.97, false, "oremagnaali "),
            (471.79, 60.89, 3.37, 7.97, false, "["),
            (475.17, 60.89, 4.49, 7.97, false, "0"),
            (479.65, 60.89, 7.94, 7.97, false, "]. "),
            (486.99, 60.89, 9.48, 7.97, false, "Qu "),
            (495.83, 60.89, 13.53, 7.97, false, "alo "),
            (508.76, 60.89, 28.12, 7.97, false, "remipsu "),
            (536.31, 60.89, 23.70, 7.97, false, "mdolo, "),
            (37.59, 54.43, 253.43, 7.97, false, "laboreet, dol orema gnaaliqualor emip Sumd olorsi tametcons ecte tura "),
            (296.21, 31.62, 3.59, 6.38, false, "9"),
            (299.80, 31.62, 1.87, 6.38, false, " "),
        ];
        let spans: Vec<TextSpan> = page
            .iter()
            .map(|&(x, y, w, size, bold, text)| {
                let mut span = make_span_text(x, y, w, size, text, size);
                if bold {
                    span.font_weight = FontWeight::Bold;
                }
                span
            })
            .collect();
        let mut context = ReadingOrderContext::new();
        if let Some(gutter) = crate::document::PdfDocument::detect_column_gutter(&spans) {
            context = context.with_column_gutter(gutter);
        }
        let ordered = strategy.apply(spans.clone(), &context).unwrap();
        let pos = |i: usize| {
            ordered
                .iter()
                .position(|o| o.span.bbox.x == spans[i].bbox.x && o.span.bbox.y == spans[i].bbox.y)
                .unwrap()
        };
        let below_table = |i: usize| spans[i].bbox.y < 340.0 && spans[i].bbox.y > 60.0;
        let left: Vec<usize> = (0..spans.len())
            .filter(|&i| below_table(i) && spans[i].bbox.right() < 295.0)
            .collect();
        let right: Vec<usize> = (0..spans.len())
            .filter(|&i| below_table(i) && spans[i].bbox.left() > 300.0)
            .collect();
        assert!(left.len() >= 5 && right.len() >= 5);
        let last_left = left.iter().map(|&i| pos(i)).max().unwrap();
        let first_right = right.iter().map(|&i| pos(i)).min().unwrap();
        assert!(last_left < first_right, "the left column is woven into the right one");
    }
}
