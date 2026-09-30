//! Utilities for splitting and analyzing PDF paragraphs.

use super::types::PdfParagraph;

/// Maximum baseline-to-baseline gap, as a multiple of the larger paragraph's
/// dominant font size, permitted when merging a continuation paragraph.
///
/// A wrapped continuation line sits roughly one line-height (leading is
/// typically 1.0–1.6× the font size) below its predecessor. A gap several times
/// larger means the two paragraphs come from spatially distinct regions — e.g. a
/// recipient block near the top of an invoice and a legal footer near the
/// bottom — and must never be joined into one logical paragraph, which would
/// associate their text (and mangle the merged block's bounding box). See #1350.
const MAX_CONTINUATION_LINE_GAP_MULTIPLE: f32 = 3.0;

/// #3013: the baseline gap, in line pitches, a paragraph that ENDS a sentence and is set
/// smaller than the lowercase paragraph under it (a note, `NOTE_FONT_SIZE_STEP` or more)
/// may have to it and still be joined. With a terminator the lowercase opening is the only
/// continuation signal, and a lowercase line that resumes a sentence from elsewhere (the
/// other column, above a table) meets it as well. A real wrap sits one pitch below its
/// predecessor; a blank line between them is a break. The pitch is the paragraphs' own
/// (the larger of the two), so a generously leaded document keeps its joins. On Soluble p3
/// the note under Table 1 (`… CRP <10 mg/L.`, pitch 9.6 pt) sits 22.4 pt above `intensified
/// treatment group …`, and was joined to it. ~keep
const MAX_TERMINATED_CONTINUATION_PITCH_MULTIPLE: f32 = 1.5;

/// #3026: two BOLD paragraphs whose sizes differ by more than this are two headings (a
/// heading and the sub-heading or callout under it), not one heading wrapping -- a wrap keeps
/// its size. Mirrors the XY-cut heading-run pre-pass's own size epsilon. ~keep
const BOLD_WRAP_MAX_SIZE_STEP: f32 = 0.5;

/// #3026: a baseline gap under this fraction of the pitch is the same printed line (a digit
/// set in its own font inside a sentence), which the size rule must not tear. ~keep
const SAME_LINE_PITCH_FRACTION: f32 = 0.5;

/// #3013: a single-line paragraph has no pitch of its own; one this many font sizes is assumed,
/// the top of the usual 1.0–1.3 range. ~keep
const ASSUMED_PITCH_FONT_FACTOR: f32 = 1.3;

/// Merge consecutive body-text paragraphs that are continuations of the same logical paragraph.
///
/// Two consecutive paragraphs are merged if:
/// - Both are body text (no heading_level, not is_list_item)
/// - The first paragraph doesn't end with sentence-ending punctuation
/// - Font sizes are within 2pt of each other
/// - Their baselines are within [`MAX_CONTINUATION_LINE_GAP_MULTIPLE`] line-heights
pub(super) fn merge_continuation_paragraphs(paragraphs: &mut Vec<PdfParagraph>) {
    if paragraphs.len() < 2 {
        return;
    }

    let old = std::mem::take(paragraphs);
    let mut iter = old.into_iter();
    let mut current = iter.next().unwrap();

    for next in iter {
        let both_body = current.heading_level.is_none()
            && next.heading_level.is_none()
            && !current.is_list_item
            && !next.is_list_item
            && !current.is_code_block
            && !next.is_code_block
            && !current.is_formula
            && !next.is_formula;
        let fonts_compatible = (current.dominant_font_size - next.dominant_font_size).abs() < 2.0;
        // Never merge across a bold-state boundary. A bold run following non-bold
        // prose (or vice versa) is a formatting break — an emphasized heading or a
        // list item's bold lead-in — not a wrapped continuation. Absorbing it would
        // bury the heading as inline bold before it can be classified. This mirrors
        // the `bold_change` paragraph break in the heuristic line grouper. ~keep
        let bold_compatible = current.is_bold == next.is_bold;
        let continuation_signal = !ends_with_sentence_terminator(&current) || starts_with_lowercase_continuation(&next);
        let same_region = current.layout_region_path == next.layout_region_path;
        let same_rotation = paragraphs_share_rotation(&current, &next);
        let vertical_gap_compatible = baselines_within_continuation_gap(&current, &next)
            && (!ends_with_sentence_terminator(&current)
                || next.dominant_font_size - current.dominant_font_size < super::pipeline::NOTE_FONT_SIZE_STEP
                || terminated_join_within_one_pitch(&current, &next));
        // A numbered section heading starts a new logical element and must never be
        // absorbed as a continuation. A heading does not end in `.?!:;`, so
        // `continuation_signal` is satisfied by the *previous* heading alone — it is
        // an `||` boost, not a requirement — and a run of consecutive subsection
        // headings would otherwise be re-joined here even after the line grouper
        // split them. See #1386. ~keep
        let next_starts_section = starts_numbered_section(&next);
        // The mirror of `next_starts_section`: a numbered section heading must never be
        // absorbed AS a continuation either. The line grouper in `blocks_to_paragraphs`
        // (`pipeline.rs`) already splits a heading from unrelated text that follows it,
        // but that split is undone here unless this pass independently refuses to
        // re-join it -- the two passes see none of each other's decisions. The single
        // exception is a heading that is itself still wrapping onto its next physical
        // line, which must stay joined; `heading_wraps_onto` is the same right-edge
        // test the grouper uses, applied to the boundary segments so both passes agree
        // on the same wrap. See #1467. ~keep
        let current_starts_section = starts_numbered_section(&current);
        // Which wrap test applies depends on the heading's own shape, exactly as in
        // the grouper: a heading set with a hanging indent is continued only by a
        // line at its title's edge that does not outrun it, and the right-edge
        // similarity is reserved for a heading with no indent to test. Without that,
        // this pass undid the grouper's split on GH#1634's manuals -- `warmtebron`
        // (ends x 135.4) and the run-in sub-heading `Werkingsprincipe` at the margin
        // (ends x 117.0) share a right edge by coincidence, and the heading the
        // grouper had just closed was re-joined to its body here. ~keep
        let next_right_edge = next
            .lines
            .iter()
            .filter_map(|line| line.segments.last())
            .map(|segment| segment.upright_advance_extent().1)
            .filter(|edge| edge.is_finite())
            .fold(f32::NEG_INFINITY, f32::max);
        let boundary_is_heading_wrap =
            super::pipeline::HeadingRun::from_lines(current.lines.iter().map(|line| line.segments.as_slice()))
                .zip(current.lines.last().and_then(|line| line.segments.last()))
                .zip(next.lines.first().and_then(|line| line.segments.first()))
                .is_some_and(|((run, prev_segment), next_segment)| {
                    let next_first_line_right = next
                        .lines
                        .first()
                        .map(|line| {
                            line.segments
                                .iter()
                                .map(|segment| segment.upright_advance_extent().1)
                                .filter(|edge| edge.is_finite())
                                .fold(f32::NEG_INFINITY, f32::max)
                        })
                        .unwrap_or(f32::NEG_INFINITY);
                    // GH#1650's margin rule is the grouper's to decide (`None`): here a
                    // body-level pair at the margin already has GH#1605's own
                    // lowercase-plus-fills-column exemption below, and a heading-level
                    // pair never reaches this pass (`both_body`). ~keep
                    run.continues_onto(prev_segment, next_segment, next_first_line_right, next_right_edge, None)
                });
        // `heading_wraps_onto` alone cannot decide this. It compares the two lines'
        // RIGHT EDGES, but its own doc comment states the correct rule -- a wrapping
        // heading "fills its column before continuing below" -- and that is a property
        // of the FIRST line. The continuation is by definition whatever is left over,
        // so its right edge is arbitrary and the two coincide only by accident.
        // Measured on GH#1605's reproducer (12pt, x=72), the metric is ANTI-correlated
        // with the answer: the page that must merge differs by 58.7pt while the page
        // that must split differs by 28.5pt, so no tolerance separates them.
        //
        // A lowercase opening tracks a wrap -- a heading continuing mid-phrase resumes
        // in lowercase -- but it is NOT sufficient on its own, and shipping it alone
        // was a mistake: body prose beginning lowercase under a COMPLETE numbered
        // heading has the same signature, and GH#1609's reproducer welded on both of
        // its pages, control included. "Widening can only merge more" was true and was
        // the wrong safety argument, because merging more is precisely that regression.
        //
        // The second conjunct is the half `heading_wraps_onto`'s doc comment always
        // claimed and never measured: a wrapping heading FILLS its column before
        // continuing below. That is a property of the heading's own line, measured
        // against the width of what would be merged onto it -- not of the continuation,
        // whose right edge is arbitrary. See [`super::pipeline::heading_fills_column`]. ~keep
        let heading_fills_column = current
            .lines
            .last()
            .and_then(|line| line.segments.last())
            .is_some_and(|prev_segment| super::pipeline::heading_fills_column(prev_segment, next_right_edge));
        let heading_wrap_exempt =
            boundary_is_heading_wrap || (starts_with_lowercase_continuation(&next) && heading_fills_column);
        // #3026: a bold paragraph never runs on into a bold paragraph of another size a line
        // or more below it. Two headings in a row -- `FINAL (…): Your Career in Business`
        // (11.04 pt) over `PART 1: PERSONAL REFLECTION:` (10.08 pt) -- pass every other test
        // here: both body-level, both bold, sizes within 2 pt, no terminator. ~keep
        let bold_size_step = current.is_bold
            && next.is_bold
            && (current.dominant_font_size - next.dominant_font_size).abs() > BOLD_WRAP_MAX_SIZE_STEP
            && below_its_own_line(&current, &next)
            && share_a_left_edge(&current, &next);
        let should_merge = !bold_size_step
            && both_body
            && fonts_compatible
            && bold_compatible
            && continuation_signal
            && same_region
            && same_rotation
            && vertical_gap_compatible
            && !next_starts_section
            && (!current_starts_section || heading_wrap_exempt);

        if should_merge {
            current.text.clear();
            current.block_bbox = union_block_bbox(current.block_bbox, next.block_bbox);
            current.lines.extend(next.lines);
        } else {
            paragraphs.push(current);
            current = next;
        }
    }

    paragraphs.push(current);
}

fn paragraphs_share_rotation(current: &PdfParagraph, next: &PdfParagraph) -> bool {
    let current_rotation = current.lines.last().and_then(|line| line.segments.last());
    let next_rotation = next.lines.first().and_then(|line| line.segments.first());
    match (current_rotation, next_rotation) {
        (Some(current), Some(next)) => current.has_same_rotation(next),
        _ => true,
    }
}

/// Whether `next`'s first baseline is close enough below `current`'s last
/// baseline to be a wrapped continuation rather than a spatially distant block.
///
/// Returns `true` when either paragraph lacks per-line geometry (`baseline_y`
/// unset on the structure-tree path), preserving the prior behavior for inputs
/// where a vertical distance cannot be computed.
/// #3013: see [`MAX_TERMINATED_CONTINUATION_PITCH_MULTIPLE`]. `true` without geometry, as
/// [`baselines_within_continuation_gap`]. ~keep
fn terminated_join_within_one_pitch(current: &PdfParagraph, next: &PdfParagraph) -> bool {
    let (Some(current_last), Some(next_first)) = (current.lines.last(), next.lines.first()) else {
        return true;
    };
    if current_last.baseline_y == 0.0 || next_first.baseline_y == 0.0 {
        return true;
    }
    let gap = (current_last.baseline_y - next_first.baseline_y).abs();
    let assumed = current.dominant_font_size.max(next.dominant_font_size).max(1.0) * ASSUMED_PITCH_FONT_FACTOR;
    let pitch = match (line_pitch(current), line_pitch(next)) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => assumed,
    };
    gap <= pitch * MAX_TERMINATED_CONTINUATION_PITCH_MULTIPLE
}

/// #3026: `next` starts on a line of its own below `current` -- its first baseline at least
/// [`SAME_LINE_PITCH_FRACTION`] of a pitch under `current`'s last. `false` without geometry,
/// so the size rule stays out of paragraphs it cannot place. ~keep
fn below_its_own_line(current: &PdfParagraph, next: &PdfParagraph) -> bool {
    let (Some(current_last), Some(next_first)) = (current.lines.last(), next.lines.first()) else {
        return false;
    };
    if current_last.baseline_y == 0.0 || next_first.baseline_y == 0.0 {
        return false;
    }
    let gap = (current_last.baseline_y - next_first.baseline_y).abs();
    let assumed = current.dominant_font_size.max(next.dominant_font_size).max(1.0) * ASSUMED_PITCH_FONT_FACTOR;
    let pitch = match (line_pitch(current), line_pitch(next)) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => assumed,
    };
    gap >= pitch * SAME_LINE_PITCH_FRACTION
}

/// #3026: `next`'s first line starts within an em of `current`'s last line -- a stack of
/// headings down one margin. Labels scattered over a figure, a map or a centred receipt
/// footer do not share an edge; splitting them only turned each label into a heading of its
/// own, so they are left to the rest of the pass. `false` without geometry. ~keep
fn share_a_left_edge(current: &PdfParagraph, next: &PdfParagraph) -> bool {
    let (Some(current_last), Some(next_first)) = (current.lines.last(), next.lines.first()) else {
        return false;
    };
    let (Some(a), Some(b)) = (current_last.segments.first(), next_first.segments.first()) else {
        return false;
    };
    let em = current.dominant_font_size.max(next.dominant_font_size).max(1.0);
    (a.x - b.x).abs() <= em
}

/// #3013: the median baseline-to-baseline advance of a paragraph's own lines. ~keep
fn line_pitch(paragraph: &PdfParagraph) -> Option<f32> {
    let mut advances: Vec<f32> = paragraph
        .lines
        .windows(2)
        .filter(|pair| pair[0].baseline_y != 0.0 && pair[1].baseline_y != 0.0)
        .map(|pair| (pair[0].baseline_y - pair[1].baseline_y).abs())
        .filter(|advance| *advance > 0.5)
        .collect();
    if advances.is_empty() {
        return None;
    }
    advances.sort_by(f32::total_cmp);
    Some(advances[advances.len() / 2])
}

fn baselines_within_continuation_gap(current: &PdfParagraph, next: &PdfParagraph) -> bool {
    let (Some(current_last), Some(next_first)) = (current.lines.last(), next.lines.first()) else {
        return true;
    };
    // No geometry available (e.g. synthesized paragraphs): fall back to allowing
    // the merge, gated by the other continuation signals.
    if current_last.baseline_y == 0.0 || next_first.baseline_y == 0.0 {
        return true;
    }
    let gap = (current_last.baseline_y - next_first.baseline_y).abs();
    let line_height = current.dominant_font_size.max(next.dominant_font_size).max(1.0);
    gap <= line_height * MAX_CONTINUATION_LINE_GAP_MULTIPLE
}

/// Union of two optional block bounding boxes in `(left, bottom, right, top)`
/// PDF-coordinate form, so a merged paragraph's box spans all of its text.
fn union_block_bbox(
    current: Option<(f32, f32, f32, f32)>,
    next: Option<(f32, f32, f32, f32)>,
) -> Option<(f32, f32, f32, f32)> {
    match (current, next) {
        (Some((cl, cb, cr, ct)), Some((nl, nb, nr, nt))) => Some((cl.min(nl), cb.min(nb), cr.max(nr), ct.max(nt))),
        (Some(bbox), None) | (None, Some(bbox)) => Some(bbox),
        (None, None) => None,
    }
}

/// Whether a paragraph's first visual line reads as a numbered section heading
/// ("1.3 Gasinstallatie", "IV. Results", "1. INTRODUCTION").
///
/// The first line's segments are re-joined because word-processor output often
/// splits the numbering into its own text run, so the leading segment alone can
/// be a bare "1.3". The conservative `is_numbered_section_heading` is used rather
/// than `starts_with_section_number` so a paragraph opening with a bare year
/// ("2024 was een druk jaar") is still treated as prose.
fn starts_numbered_section(para: &PdfParagraph) -> bool {
    let Some(first_line) = para.lines.first() else {
        return false;
    };
    let joined = first_line
        .segments
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    super::classify::is_numbered_section_heading(&joined)
}

/// Check if a paragraph starts with a lowercase letter, indicating it's a
/// continuation of a previous sentence split across paragraph boundaries.
fn starts_with_lowercase_continuation(para: &PdfParagraph) -> bool {
    let first_text = para
        .lines
        .first()
        .and_then(|l| l.segments.first())
        .map(|s| s.text.trim_start())
        .unwrap_or("");
    first_text.chars().next().is_some_and(|c| c.is_lowercase())
}

/// Check if a paragraph's last line ends with sentence-terminating punctuation.
///
/// Used by merge_continuation_paragraphs to determine if two consecutive
/// paragraphs should be merged. Supports ASCII and CJK sentence terminators.
fn ends_with_sentence_terminator(para: &PdfParagraph) -> bool {
    let last_text = para
        .lines
        .last()
        .and_then(|l| l.segments.last())
        .map(|s| s.text.trim_end())
        .unwrap_or("");
    matches!(
        last_text.chars().last(),
        Some('.' | '?' | '!' | ':' | ';' | '\u{3002}' | '\u{FF1F}' | '\u{FF01}')
    )
}

/// Split paragraphs that contain embedded bullet characters (e.g. `•`) into separate list items.
///
/// Structure tree pages sometimes merge all text into one block with inline bullets.
/// This splits "text before • item1 • item2" into separate paragraphs.
pub(super) fn split_embedded_list_items(paragraphs: &mut Vec<PdfParagraph>) {
    let old = std::mem::take(paragraphs);
    for para in old {
        if para.heading_level.is_some() || para.is_list_item || para.is_code_block || para.is_formula {
            paragraphs.push(para);
            continue;
        }

        let full_text: String = para
            .lines
            .iter()
            .flat_map(|l| l.segments.iter())
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let bullet_count = full_text.matches(['\u{2022}', '\u{00B7}']).count();
        if bullet_count < 2 {
            paragraphs.push(para);
            continue;
        }

        let font_size = para.dominant_font_size;
        let is_bold = para.is_bold;

        let parts: Vec<&str> = full_text.split(['\u{2022}', '\u{00B7}']).collect();
        let before = parts[0].trim().trim_end_matches('\u{00C2}').trim();
        if !before.is_empty() {
            paragraphs.push(text_to_paragraph(before, font_size, is_bold, false));
        }
        for part in &parts[1..] {
            let item_text = part
                .trim()
                .trim_start_matches('\u{00C2}')
                .trim_end_matches('\u{00C2}')
                .trim();
            if !item_text.is_empty() {
                paragraphs.push(text_to_paragraph(item_text, font_size, is_bold, true));
            }
        }
    }
}

/// Create a simple paragraph from text.
fn text_to_paragraph(text: &str, font_size: f32, is_bold: bool, is_list_item: bool) -> PdfParagraph {
    use crate::pdf::hierarchy::SegmentData;

    let segments: Vec<SegmentData> = text
        .split_whitespace()
        .map(|w| SegmentData {
            text: w.to_string(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            font_size,
            is_bold,
            is_italic: false,
            is_monospace: false,
            baseline_y: 0.0,
            rotation_degrees: 0.0,
            assigned_role: None,
        })
        .collect();

    let line = super::types::PdfLine {
        segments,
        baseline_y: 0.0,
        dominant_font_size: font_size,
        is_bold,
        is_monospace: false,
    };

    let lines = vec![line];
    let word_count = PdfParagraph::compute_word_count("", &lines);
    PdfParagraph {
        text: String::new(),
        lines,
        dominant_font_size: font_size,
        heading_level: None,
        is_bold,
        is_list_item,
        is_code_block: false,
        is_formula: false,
        is_page_furniture: false,
        layout_class: None,
        layout_region_path: None,
        caption_for: None,
        block_bbox: None,
        word_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_body_paragraph(text: &str, font_size: f32) -> PdfParagraph {
        use crate::pdf::hierarchy::SegmentData;

        let segments = vec![SegmentData {
            text: text.to_string(),
            x: 0.0,
            y: 700.0,
            width: 200.0,
            height: font_size,
            font_size,
            is_bold: false,
            is_italic: false,
            is_monospace: false,
            baseline_y: 700.0,
            rotation_degrees: 0.0,
            assigned_role: None,
        }];

        let lines = vec![super::super::types::PdfLine {
            segments,
            baseline_y: 700.0,
            dominant_font_size: font_size,
            is_bold: false,
            is_monospace: false,
        }];
        let word_count = PdfParagraph::compute_word_count("", &lines);
        PdfParagraph {
            text: String::new(),
            lines,
            dominant_font_size: font_size,
            heading_level: None,
            is_bold: false,
            is_list_item: false,
            is_code_block: false,
            is_formula: false,
            is_page_furniture: false,
            layout_class: None,
            layout_region_path: None,
            caption_for: None,
            block_bbox: None,
            word_count,
        }
    }

    #[test]
    fn should_not_merge_paragraphs_across_rotation_boundary() {
        let mut rotated = make_body_paragraph("Engine oil need only meet the", 12.0);
        rotated.lines[0].segments[0].rotation_degrees = 90.0;
        let footer = make_body_paragraph("vehicle footer", 12.0);
        let mut paragraphs = vec![rotated, footer];

        merge_continuation_paragraphs(&mut paragraphs);

        assert_eq!(paragraphs.len(), 2);
    }

    fn make_body_paragraph_at(text: &str, font_size: f32, baseline_y: f32) -> PdfParagraph {
        let mut para = make_body_paragraph(text, font_size);
        para.lines[0].baseline_y = baseline_y;
        if let Some(segment) = para.lines[0].segments.first_mut() {
            segment.baseline_y = baseline_y;
            segment.y = baseline_y;
        }
        para
    }

    /// #3013 (Soluble p3): the note under a table ends a sentence; the lowercase line a blank
    /// line below it resumes a sentence from the other column, not the note's. ~keep
    #[test]
    fn a_terminated_paragraph_does_not_absorb_a_lowercase_paragraph_a_blank_line_below_3013() {
        let mut paragraphs = vec![
            make_body_paragraph_at(
                "Reference ranges for creatinine 60-115 umol/L, CRP <10 mg/L.",
                8.0,
                227.9,
            ),
            make_body_paragraph_at("intensified treatment group and the standard-of-care group", 9.0, 205.5),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 2);
    }

    /// A bold paragraph of `lines` (`(text, font size, baseline)`), for #3026. ~keep
    fn make_bold_paragraph(lines: &[(&str, f32, f32)]) -> PdfParagraph {
        let mut para = make_body_paragraph_at(lines[0].0, lines[0].1, lines[0].2);
        let template = para.lines[0].clone();
        para.lines.clear();
        for &(text, size, baseline) in lines {
            let mut line = template.clone();
            line.baseline_y = baseline;
            line.dominant_font_size = size;
            line.is_bold = true;
            line.segments[0].text = text.to_string();
            line.segments[0].font_size = size;
            line.segments[0].height = size;
            line.segments[0].baseline_y = baseline;
            line.segments[0].y = baseline;
            line.segments[0].is_bold = true;
            para.lines.push(line);
        }
        para.is_bold = true;
        para.dominant_font_size = lines[0].1;
        para
    }

    /// #3026 (f8d3a162 p13, geometry verbatim, text lorem ipsum): an 11.04 pt bold heading and
    /// the 10.08 pt two-line bold heading 19.2 pt under it are two headings. ~keep
    #[test]
    fn a_bold_heading_does_not_absorb_a_smaller_bold_heading_below_it_3026() {
        let mut paragraphs = vec![
            make_bold_paragraph(&[(
                "LOREM (Ipsumdolo si Ame Tconse ct 3rd EIU): Smod Tempor in Cididunt",
                11.04,
                177.84,
            )]),
            make_bold_paragraph(&[("LORE 1:", 10.08, 158.64), ("IPSUMDOL ORSITAMETC:", 10.08, 147.12)]),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 2);
    }

    /// #3026's control: two bold labels of different sizes that do not share a left edge
    /// (Wisselaar reparatieset p1: the 9 pt label `niet afdichten` at x 121.3 and the 10 pt
    /// `Lokaliseer de haarscheur` at x 289.7, 26.2 pt apart) are not split by the size
    /// rule. ~keep
    #[test]
    fn bold_labels_on_different_left_edges_are_left_to_the_other_rules_3026() {
        let mut label = make_bold_paragraph(&[("Lorem ipsumdo", 9.0, 741.94)]);
        label.lines[0].segments[0].x = 121.31;
        let mut other = make_bold_paragraph(&[("Doloremips ut al laboreetd", 10.0, 768.14)]);
        other.lines[0].segments[0].x = 289.72;
        let mut paragraphs = vec![label, other];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1);
    }

    /// #3026's control: a bold heading wrapping onto a second line of its own size still
    /// joins (f8d3a162, `FINAL ASSESSMENT: …` over `INTERNSHIP W/ …`, 10.08 pt, 11.52 pt). ~keep
    #[test]
    fn a_bold_heading_still_takes_its_wrap_of_the_same_size_3026() {
        let mut paragraphs = vec![
            make_bold_paragraph(&[("LOREM IPSUMDOLO: SITA-METC CONSECT", 10.08, 300.0)]),
            make_bold_paragraph(&[("ETURADIPIS C/ ELITSE & DOEIUSM TEMPORINCI DIDUNTUT", 10.08, 288.48)]),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1);
    }

    /// #3026's control: a digit set in its own, larger font inside a bold sentence sits on the
    /// sentence's own baseline (88557804: 9.48 pt text, 11.04 pt LCD digit, gap 0) and is not
    /// torn off by the size rule. ~keep
    #[test]
    fn a_larger_digit_on_the_same_line_is_not_torn_off_3026() {
        let mut paragraphs = vec![
            make_bold_paragraph(&[(
                "Lo remipsum dolo si tamet co nsectetu ra dip is ci ngelits van",
                9.48,
                412.0,
            )]),
            make_bold_paragraph(&[("0' seddoeius (modtemp orincid).", 11.04, 412.0)]),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1);
    }

    /// #3013's control: a lowercase line one leading below a terminated line is still its wrap. ~keep
    #[test]
    fn a_terminated_paragraph_still_takes_a_lowercase_wrap_one_leading_below_3013() {
        let mut paragraphs = vec![
            make_body_paragraph_at("The dose was 5 mg/kg i.v.", 9.0, 227.9),
            make_body_paragraph_at("twice daily for the first week", 9.0, 217.5),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1);
    }

    #[test]
    fn test_no_merge_across_distant_regions() {
        // A buyer tax ID in the upper recipient block and a seller tax ID in the
        // page footer are ~590pt apart. Neither ends with a sentence terminator,
        // so the older heuristic would merge them; the vertical-gap guard must
        // keep them separate. Regression for #1350.
        let mut paragraphs = vec![
            make_body_paragraph_at("Buyer tax ID SYNTH-BUYER-TAX-359370919", 8.0, 185.5),
            make_body_paragraph_at("Seller tax ID SYNTH-SELLER-TAX-815876165", 8.0, 775.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            2,
            "paragraphs from spatially distant regions must not merge"
        );
    }

    #[test]
    fn test_merge_adjacent_lines_within_gap() {
        // Genuine wrapped continuation one line-height apart still merges.
        let mut paragraphs = vec![
            make_body_paragraph_at("The committee reviewed the annual", 11.0, 712.0),
            make_body_paragraph_at("report and approved the budget", 11.0, 698.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1, "adjacent continuation lines should merge");
    }

    #[test]
    fn test_merge_unions_block_bbox() {
        let mut upper = make_body_paragraph_at("first line without terminator", 11.0, 712.0);
        upper.block_bbox = Some((60.0, 705.0, 260.0, 720.0));
        let mut lower = make_body_paragraph_at("second line continues", 11.0, 698.0);
        lower.block_bbox = Some((60.0, 691.0, 300.0, 706.0));
        let mut paragraphs = vec![upper, lower];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1, "adjacent lines should merge");
        assert_eq!(
            paragraphs[0].block_bbox,
            Some((60.0, 691.0, 300.0, 720.0)),
            "merged block bbox must span both source boxes"
        );
    }

    #[test]
    fn test_merge_lowercase_continuation() {
        let mut paragraphs = vec![
            make_body_paragraph("The regulation requires.", 12.0),
            make_body_paragraph("and all operators must comply", 12.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1, "lowercase continuation should be merged");
    }

    #[test]
    fn test_no_merge_different_font_sizes() {
        let mut paragraphs = vec![
            make_body_paragraph("First paragraph", 12.0),
            make_body_paragraph("second paragraph", 16.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 2, "different font sizes should prevent merge");
    }

    #[test]
    fn test_merge_no_terminator() {
        let mut paragraphs = vec![
            make_body_paragraph("The regulation requires", 12.0),
            make_body_paragraph("All operators must comply", 12.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1, "unterminated paragraph should merge with next");
    }

    #[test]
    fn test_no_merge_terminated_uppercase() {
        let mut paragraphs = vec![
            make_body_paragraph("The regulation requires compliance.", 12.0),
            make_body_paragraph("All operators must comply", 12.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            2,
            "terminated paragraph + uppercase start should not merge"
        );
    }

    #[test]
    fn test_no_merge_across_bold_boundary() {
        // A bold header following unterminated body prose must not be absorbed as
        // a continuation — it should survive as its own paragraph for classification. ~keep
        let body = make_body_paragraph(
            "here is also available other sources of this Manual MetcalUser Guide",
            12.0,
        );
        let mut header = make_body_paragraph("Impaired Glucose Tolerance And Impaired Fasting Glucose ...", 12.0);
        header.is_bold = true;
        let mut paragraphs = vec![body, header];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 2, "bold header must not merge into non-bold prose");
        assert!(paragraphs[1].is_bold, "the bold header paragraph must be preserved");
    }

    /// Text of a paragraph's first line, joined from its segments.
    fn first_line_text(para: &PdfParagraph) -> String {
        para.lines
            .first()
            .map(|line| {
                line.segments
                    .iter()
                    .map(|s| s.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    }

    /// Regression for #1386 (defect #290): a run of consecutive numbered
    /// subsection headings at the same size and weight, one line-height apart,
    /// must survive as four separate paragraphs. None of them ends in `.?!:;`,
    /// so `continuation_signal` alone would merge the whole run into one element
    /// and a consumer deriving a section hierarchy would see only "1.3".
    #[test]
    fn should_not_merge_consecutive_numbered_section_headings() {
        let mut paragraphs = vec![
            make_body_paragraph_at("1.3 Gasinstallatie", 11.0, 700.0),
            make_body_paragraph_at("1.4 Elektrische installatie", 11.0, 686.0),
            make_body_paragraph_at("1.5 Waterinstallatie", 11.0, 672.0),
            make_body_paragraph_at("1.6 Ventilatie", 11.0, 658.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);

        assert_eq!(
            paragraphs.len(),
            4,
            "each numbered subsection heading must remain a separate element"
        );
        assert_eq!(first_line_text(&paragraphs[0]), "1.3 Gasinstallatie");
        assert_eq!(first_line_text(&paragraphs[1]), "1.4 Elektrische installatie");
        assert_eq!(first_line_text(&paragraphs[2]), "1.5 Waterinstallatie");
        assert_eq!(first_line_text(&paragraphs[3]), "1.6 Ventilatie");
    }

    /// The over-fire guard for #1386: prose whose first token is a bare year is
    /// NOT a section heading and must still merge with the unterminated line
    /// above it. `starts_with_section_number` returns `true` here — using it
    /// instead of `is_numbered_section_heading` would split ordinary prose.
    #[test]
    fn should_merge_prose_paragraph_starting_with_a_year() {
        let mut paragraphs = vec![
            make_body_paragraph_at("Het bestuur meldt", 11.0, 700.0),
            make_body_paragraph_at("2024 was een druk jaar", 11.0, 686.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);

        assert_eq!(
            paragraphs.len(),
            1,
            "prose beginning with a bare year is not a section heading and must stay one paragraph"
        );
        assert_eq!(paragraphs[0].lines.len(), 2, "both prose lines must be present");
    }

    /// Roman-numeral and single-level ALL-CAPS headings are the other two shapes
    /// `is_numbered_section_heading` accepts; both must block a merge.
    #[test]
    fn should_not_merge_roman_or_allcaps_numbered_headings() {
        let mut paragraphs = vec![
            make_body_paragraph_at("III. Scope", 11.0, 700.0),
            make_body_paragraph_at("IV. Results", 11.0, 686.0),
            make_body_paragraph_at("5. CONCLUSIONS", 11.0, 672.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);

        assert_eq!(
            paragraphs.len(),
            3,
            "roman and ALL-CAPS numbered headings must not merge"
        );
        assert_eq!(first_line_text(&paragraphs[0]), "III. Scope");
        assert_eq!(first_line_text(&paragraphs[1]), "IV. Results");
        assert_eq!(first_line_text(&paragraphs[2]), "5. CONCLUSIONS");
    }

    /// A single-level number followed by mixed-case text is a LIST item, not a
    /// section heading, so the guard must not fire on it — this pins the
    /// conservative boundary of the predicate the fix relies on.
    #[test]
    fn should_still_merge_single_level_mixed_case_numbering() {
        let mut paragraphs = vec![
            make_body_paragraph_at("De procedure verloopt als volgt", 11.0, 700.0),
            make_body_paragraph_at("1. Eerste stap in het proces", 11.0, 686.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);

        assert_eq!(
            paragraphs.len(),
            1,
            "'1. Eerste stap' is list-shaped, not a section heading; the guard must not fire"
        );
    }

    #[test]
    fn test_starts_with_lowercase_continuation_fn() {
        let para_lower = make_body_paragraph("and furthermore", 12.0);
        assert!(starts_with_lowercase_continuation(&para_lower));

        let para_upper = make_body_paragraph("Furthermore", 12.0);
        assert!(!starts_with_lowercase_continuation(&para_upper));
    }

    #[test]
    fn test_merge_clears_precomputed_text_on_heuristic_path() {
        let mut p1 = make_body_paragraph("een indicative", 12.0);
        p1.text = "een indicative".to_string();
        let mut p2 = make_body_paragraph("van toenemende merkbekendheid", 12.0);
        p2.text = "van toenemende merkbekendheid".to_string();
        let mut paragraphs = vec![p1, p2];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1, "lowercase continuation should merge");
        assert!(
            paragraphs[0].text.is_empty(),
            "merged paragraph must clear pre-computed text so assembly joins from segments"
        );
        assert_eq!(paragraphs[0].lines.len(), 2, "both lines must be present after merge");
    }

    #[test]
    fn test_merge_struct_tree_path_text_stays_empty() {
        let mut paragraphs = vec![
            make_body_paragraph("first sentence without terminator", 12.0),
            make_body_paragraph("second continues here", 12.0),
        ];
        assert!(paragraphs[0].text.is_empty());
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1);
        assert!(paragraphs[0].text.is_empty());
    }

    /// GH#1605. A numbered heading wrapping onto a SHORT continuation line was
    /// split in two, losing the section number from the second half.
    ///
    /// `heading_wraps_onto` gates the merge on the two lines' right edges landing
    /// within 2.0 font-sizes of each other. Its own doc comment states the correct
    /// rule -- a wrapping heading "fills its column before continuing below" --
    /// but that is a property of the FIRST line; the code instead measures the
    /// SECOND line's length, and a continuation is by definition whatever is left
    /// over. Measured from the reproducer at 12pt Helvetica-Bold, x=72:
    ///
    ///   page 1  |307.5 - 248.8| = 58.7pt  must MERGE  (tolerance 24pt -> split)
    ///   page 2  |307.5 - 325.5| = 18.0pt  must MERGE  (tolerance 24pt -> merge)
    ///   page 3  |374.2 - 402.7| = 28.5pt  must SPLIT  (tolerance 24pt -> split)
    ///
    /// The page that must merge has a LARGER edge difference than the page that
    /// must split, so the metric is anti-correlated with the answer and no
    /// tolerance can separate the three. ~keep
    fn wrapped_heading_paragraph(text: &str, x: f32, right_edge: f32, baseline_y: f32) -> PdfParagraph {
        use crate::pdf::hierarchy::SegmentData;
        let segments = vec![SegmentData {
            text: text.to_string(),
            x,
            y: baseline_y,
            width: right_edge - x,
            height: 12.0,
            font_size: 12.0,
            is_bold: true,
            is_italic: false,
            is_monospace: false,
            baseline_y,
            rotation_degrees: 0.0,
            assigned_role: None,
        }];
        let lines = vec![super::super::types::PdfLine {
            segments,
            baseline_y,
            dominant_font_size: 12.0,
            is_bold: true,
            is_monospace: false,
        }];
        let word_count = PdfParagraph::compute_word_count("", &lines);
        PdfParagraph {
            text: String::new(),
            lines,
            dominant_font_size: 12.0,
            heading_level: None,
            is_bold: true,
            is_list_item: false,
            is_code_block: false,
            is_formula: false,
            is_page_furniture: false,
            layout_class: None,
            layout_region_path: None,
            caption_for: None,
            block_bbox: None,
            word_count,
        }
    }

    #[test]
    fn numbered_heading_wrapping_onto_a_short_line_stays_one_paragraph() {
        let mut paragraphs = vec![
            wrapped_heading_paragraph("2.4 Aandachtspunten ten behoeve van de", 72.0, 307.5, 700.0),
            wrapped_heading_paragraph("watertechnische installatie", 72.0, 248.8, 684.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            1,
            "GH#1605: a heading wrapping onto a short continuation must stay one paragraph; \
             the continuation starts lowercase and carries no section number of its own"
        );
    }

    #[test]
    fn numbered_heading_wrapping_onto_a_long_line_stays_one_paragraph() {
        let mut paragraphs = vec![
            wrapped_heading_paragraph("2.4 Aandachtspunten ten behoeve van de", 72.0, 307.5, 700.0),
            wrapped_heading_paragraph("watertechnische installatie en de meting", 72.0, 325.5, 684.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            1,
            "GH#1605 control: this case already merged and must keep merging"
        );
    }

    #[test]
    fn numbered_heading_followed_by_unrelated_capitalised_text_still_splits() {
        let mut paragraphs = vec![
            wrapped_heading_paragraph("1.1.1 Pictogrammen in het installatievoorschrift", 72.0, 374.2, 700.0),
            wrapped_heading_paragraph("VOORZICHTIG / BELANGRIJK Procedures die schade", 72.0, 402.7, 684.0),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            2,
            "GH#1605 negative control: unrelated capitalised text after a heading must NOT be absorbed"
        );
    }

    /// A paragraph of several segments on several lines, for the hanging-indent
    /// shape: `(text, x, right edge, baseline)` per segment, grouped into lines by
    /// baseline.
    fn hanging_paragraph(segments: &[(&str, f32, f32, f32)]) -> PdfParagraph {
        use crate::pdf::hierarchy::SegmentData;
        let mut lines: Vec<super::super::types::PdfLine> = Vec::new();
        for &(text, x, right_edge, baseline_y) in segments {
            let segment = SegmentData {
                text: text.to_string(),
                x,
                y: baseline_y,
                width: right_edge - x,
                height: 11.0,
                font_size: 11.0,
                is_bold: true,
                is_italic: false,
                is_monospace: false,
                baseline_y,
                rotation_degrees: 0.0,
                assigned_role: None,
            };
            match lines.last_mut() {
                Some(line) if (line.baseline_y - baseline_y).abs() < 0.5 => line.segments.push(segment),
                _ => lines.push(super::super::types::PdfLine {
                    segments: vec![segment],
                    baseline_y,
                    dominant_font_size: 11.0,
                    is_bold: true,
                    is_monospace: false,
                }),
            }
        }
        let word_count = PdfParagraph::compute_word_count("", &lines);
        PdfParagraph {
            text: String::new(),
            lines,
            dominant_font_size: 11.0,
            heading_level: None,
            is_bold: true,
            is_list_item: false,
            is_code_block: false,
            is_formula: false,
            is_page_furniture: false,
            layout_class: None,
            layout_region_path: None,
            caption_for: None,
            block_bbox: None,
            word_count,
        }
    }

    /// GH#1634, the installation-manual shape: the grouper closes the hanging
    /// heading after its wrap `warmtebron`, and this pass must not re-join it to
    /// the run-in sub-heading beneath -- which returns to the MARGIN, and only
    /// happens to end within two font-sizes of the wrap's right edge.
    #[test]
    fn hanging_heading_is_not_rejoined_to_a_margin_line_with_a_similar_right_edge() {
        let mut paragraphs = vec![
            hanging_paragraph(&[
                ("4.4.2", 48.2, 63.4, 775.2),
                (
                    "Opdeling CV-installatie in groepen bij aanwezigheid extra",
                    83.6,
                    329.4,
                    775.2,
                ),
                ("warmtebron", 83.6, 137.8, 762.6),
            ]),
            hanging_paragraph(&[
                ("Werkingsprincipe", 48.2, 119.3, 735.1),
                ("Indien de kamerthermostaat het toestel uitschakelt", 48.2, 354.4, 720.6),
            ]),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            2,
            "a line at the margin is not the continuation of a hanging-indent heading, \
             whatever its right edge"
        );
        assert_eq!(
            first_line_text(&paragraphs[1]).trim(),
            "Werkingsprincipe",
            "the sub-heading must open the second paragraph"
        );
    }

    /// The same heading with its genuine wrap: the wrap resumes at the title's edge
    /// and is shorter than the line it continues, so it must still be joined here
    /// when the grouper has left it as its own paragraph.
    #[test]
    fn hanging_heading_is_still_rejoined_to_its_own_wrap() {
        let mut paragraphs = vec![
            hanging_paragraph(&[
                ("4.4.2", 48.2, 63.4, 775.2),
                (
                    "Opdeling CV-installatie in groepen bij aanwezigheid extra",
                    83.6,
                    329.4,
                    775.2,
                ),
            ]),
            hanging_paragraph(&[("warmtebron", 83.6, 137.8, 762.6)]),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(paragraphs.len(), 1, "the wrap at the title's edge is the heading's own");
    }

    /// GH#1609: body prose beginning lowercase under a COMPLETE numbered heading
    /// carries the same lowercase signature as a wrap, and the first version of the
    /// GH#1605 fix merged it -- on the reporter's control page as well as the
    /// defective one. The heading's own line separates the two cases: a wrap fills
    /// its column, and this heading stops far short of the body's width.
    #[test]
    fn numbered_heading_followed_by_wider_lowercase_body_still_splits() {
        let mut paragraphs = vec![
            wrapped_heading_paragraph("3.1.7 Innovatie/ontwikkelingen", 104.42, 230.0, 700.0),
            wrapped_heading_paragraph(
                "innovatie ontwikkelingen toekomstige verwachten gebied",
                104.42,
                500.0,
                688.0,
            ),
        ];
        merge_continuation_paragraphs(&mut paragraphs);
        assert_eq!(
            paragraphs.len(),
            2,
            "a complete heading must not absorb wider body prose merely because it opens lowercase"
        );
    }
}
