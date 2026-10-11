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
/// so its offsets keep deciding.
#[test]
fn a_tracked_tj_array_without_space_characters_keeps_its_offset_breaks() {
    let stream = tracked_tj(&["lorem", "ipsum"], -170, Some(-170));
    assert_ne!(text_of(&stream), "loremipsum");
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
