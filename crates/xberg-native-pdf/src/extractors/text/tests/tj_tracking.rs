//! A `TJ` array that letter-spaces its text with one identical offset after
//! every glyph, and writes its word ends as space characters, is read as words:
//! the repeated offset is spacing, not a word boundary.

use super::super::*;
use super::extraction_and_rotation::create_test_font;

/// `[(l)-170(o)-170 …]`: one glyph per string and the same offset after each.
fn tracked_tj(words: &[&str], tracking: i32, word_offset: Option<i32>) -> String {
    let mut parts = Vec::new();
    for (w, word) in words.iter().enumerate() {
        if w > 0 {
            match word_offset {
                Some(offset) => parts.push(format!("{offset}")),
                None => parts.push(format!("( ){tracking}")),
            }
        }
        let glyphs: Vec<String> = word.chars().map(|c| format!("({c})")).collect();
        parts.push(glyphs.join(&format!("{tracking}")));
    }
    format!("BT /F1 10 Tf 100 700 Td [{}] TJ ET", parts.join(""))
}

fn text_of(stream: &str) -> String {
    let mut extractor = TextExtractor::new();
    extractor.add_font("F1".to_string(), create_test_font());
    let spans = extractor.extract_text_spans(stream.as_bytes()).unwrap();
    let text: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The geometry of a justified installation manual: 0.17 em after every glyph,
/// a space character between the words. It read `l o r e m i p s u m`.
#[test]
fn a_tracked_tj_array_with_space_characters_is_read_as_words() {
    let stream = tracked_tj(&["lorem", "ipsum", "dolor", "sit"], -170, None);
    assert_eq!(text_of(&stream), "lorem ipsum dolor sit");
}

/// An offset clearly wider than the tracking is still a word break.
#[test]
fn a_wider_offset_in_a_tracked_array_still_breaks_words() {
    let mut stream = tracked_tj(&["lorem", "ipsum", "dolor"], -170, None);
    stream = stream
        .replace("(m)-170(i)", "(m)-900(i)")
        .replace("( )-170(i)", "(z)-900(i)");
    let text = text_of(&stream);
    assert!(
        text.contains("loremz ipsum"),
        "the -900 offset must stay a break: {text}"
    );
}

/// Without a single space character the array has not said where its words end,
/// so a wider offset among the tracked ones keeps it from being one word (X45).
#[test]
fn a_tracked_tj_array_without_space_characters_keeps_its_offset_breaks() {
    let stream = tracked_tj(&["lorem", "ipsum"], -170, Some(-600));
    assert_ne!(text_of(&stream), "loremipsum");
}

/// `[(D)-130(E)-139 …]`: a single glyph per string and nothing else between
/// them.
fn tracked_word(glyphs: &str, offsets: &[i32]) -> String {
    let mut parts = Vec::new();
    for (i, c) in glyphs.chars().enumerate() {
        if i > 0 {
            parts.push(format!("{}", offsets[(i - 1) % offsets.len()]));
        }
        parts.push(format!("({c})"));
    }
    format!("BT /F1 10 Tf 100 700 Td [{}] TJ ET", parts.join(""))
}

/// X45 -- a contract draws each chapter title as one tracked word, with no
/// space character in the array; it read `D E F I N I T I O N S`.
#[test]
fn a_tracked_single_word_without_space_characters_is_one_word() {
    let stream = tracked_word("DEFINITIONS", &[-130, -139, -132, -135, -134, -127]);
    assert_eq!(text_of(&stream), "DEFINITIONS");
}

/// X45 -- five offsets are below X31's six, and still one word.
#[test]
fn a_short_tracked_word_is_one_word() {
    let stream = tracked_word("RETURN", &[-131, -139, -127, -135]);
    assert_eq!(text_of(&stream), "RETURN");
}

/// X45 -- the chapter number in front of it is tracked the same way: `1 0 .`.
#[test]
fn a_tracked_section_number_closed_by_its_dot_is_one_word() {
    assert_eq!(text_of(&tracked_word("10.", &[-137, -133])), "10.");
    assert_eq!(text_of(&tracked_word("3.2.", &[-130])), "3.2.");
}

/// X45 -- digits without the closing dot are labels: a drawing's grid row stays
/// spaced, with or without a space character.
#[test]
fn a_tracked_row_of_digits_without_a_dot_keeps_its_spaces() {
    assert!(
        text_of(&tracked_word("87654321", &[-170])).starts_with("8 7 6"),
        "grid labels must keep their spaces"
    );
}

/// X45 -- three spaced capitals can be initials; a word needs four letters.
#[test]
fn three_tracked_capitals_keep_their_spaces() {
    assert!(text_of(&tracked_word("ABC", &[-170])).starts_with("A B C"));
}

/// X45 -- an offset wider than letter-spacing is a column, not tracking, and a
/// spaceless array with one offset off the median is not one word.
#[test]
fn wide_or_uneven_offsets_without_spaces_keep_their_breaks() {
    assert!(text_of(&tracked_word("ABCDEF", &[-600])).starts_with("A B C"));
    assert_ne!(
        text_of(&tracked_word("ABCDEF", &[-130, -130, -400, -130, -130])),
        "ABCDEF"
    );
}

/// Ordinary kerning with one real word offset is not tracking.
#[test]
fn an_ordinary_kerned_array_keeps_its_word_offset() {
    let stream = "BT /F1 10 Tf 100 700 Td [(Lor)-20(em)-600(ipsum)-15( dolor)] TJ ET";
    assert_eq!(text_of(stream), "Lorem ipsum dolor");
}

/// A drawing's grid row is digits spaced out on purpose; even with a space
/// character in the array it is not tracked text.
#[test]
fn an_evenly_spaced_row_of_digits_is_not_tracking() {
    let stream = tracked_tj(&["87654321", "HH"], -170, None);
    assert!(
        text_of(&stream).starts_with("8 7 6"),
        "grid labels must keep their spaces"
    );
}
