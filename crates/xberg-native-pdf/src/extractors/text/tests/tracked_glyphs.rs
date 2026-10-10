//! The tracked-glyph guard in `merge_adjacent_spans`: a producer that draws one
//! glyph per show-text operator and writes word ends as explicit space glyphs
//! has already said where its words end, so the uniform gap of a letter-spaced
//! line is not a word break.

use super::super::*;

const FONT_SIZE: f32 = 20.0;
const BASELINE: f32 = 700.0;
const GLYPH_WIDTH: f32 = 12.0;
const SPACE_WIDTH: f32 = 5.0;

/// One span per glyph, laid out left to right from `x` with `gap` points between
/// the boxes; a `' '` becomes an explicit space-glyph span.
fn glyph_line(text: &str, x: f32, y: f32, gap: f32) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut at = x;
    for ch in text.chars() {
        let width = if ch == ' ' { SPACE_WIDTH } else { GLYPH_WIDTH };
        spans.push(TextSpan {
            text: ch.to_string(),
            bbox: Rect::new(at, y, width, FONT_SIZE),
            font_name: "F1".to_string(),
            font_size: FONT_SIZE,
            ..TextSpan::default()
        });
        at += width + gap;
    }
    spans
}

fn merged(spans: Vec<TextSpan>) -> Vec<String> {
    let mut extractor = TextExtractor::new();
    extractor.spans = spans
        .into_iter()
        .enumerate()
        .map(|(i, s)| TextSpan { sequence: i, ..s })
        .collect();
    extractor.merge_adjacent_spans();
    extractor.spans.iter().map(|s| s.text.clone()).collect()
}

/// A cover title set with wide letter-spacing (0.3 em between every glyph box),
/// drawn one glyph at a time, its word ends written as space glyphs -- the
/// geometry of a case-study cover whose title read `C A S E S T U D Y`.
#[test]
fn a_letter_spaced_title_drawn_glyph_by_glyph_is_read_as_words() {
    let mut spans = glyph_line("LOREM IPSUM DOLOR", 50.0, BASELINE, 6.0);
    spans.extend(glyph_line("SIT AMET", 50.0, BASELINE - 40.0, 6.0));
    assert_eq!(merged(spans), vec!["LOREM IPSUM DOLOR", "SIT AMET"]);
}

/// The page signal is what makes a line without a space glyph of its own count:
/// a one-word tracked line under a tracked line that has space glyphs.
#[test]
fn a_one_word_tracked_line_reads_whole_on_a_page_with_space_glyphs() {
    let mut spans = glyph_line("LOREM IPSUM DOLOR SIT", 50.0, BASELINE, 6.0);
    spans.extend(glyph_line("CONSECTETUR", 50.0, BASELINE - 40.0, 6.0));
    assert_eq!(merged(spans), vec!["LOREM IPSUM DOLOR SIT", "CONSECTETUR"]);
}

/// Without a single space glyph on the page the producer has not said where its
/// words end, so the gaps keep deciding.
#[test]
fn a_page_without_space_glyphs_keeps_its_gap_spaces() {
    let spans = glyph_line("LOREMIPSUM", 50.0, BASELINE, 6.0);
    let text = merged(spans).join("");
    assert_ne!(
        text, "LOREMIPSUM",
        "no explicit space glyph, so the guard must not fire"
    );
}

/// Ordinary text drawn glyph by glyph has near-zero letter gaps; a wider gap on
/// such a line is a real word break even on a page with space glyphs.
#[test]
fn an_untracked_line_keeps_a_word_gap_without_a_space_glyph() {
    let mut spans = glyph_line("LOREM IPSUM DOLOR", 50.0, BASELINE, 0.5);
    // `SIT` and `AMET` drawn with no space glyph between them, 8 pt apart.
    let mut sit = glyph_line("SIT", 50.0, BASELINE - 40.0, 0.5);
    let amet_x = sit.last().map(|s| s.bbox.x + s.bbox.width + 8.0).unwrap_or(0.0);
    sit.extend(glyph_line("AMET", amet_x, BASELINE - 40.0, 0.5));
    spans.extend(sit);
    assert_eq!(merged(spans), vec!["LOREM IPSUM DOLOR", "SIT AMET"]);
}
