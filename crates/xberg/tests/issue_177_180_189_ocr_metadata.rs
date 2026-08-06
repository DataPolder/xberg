#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: org logging policy exempts tests
//! Integration coverage for the Tesseract OCR execution-path fixes:
//!
//! - #177: table detection now clusters words into independent spatial regions
//!   instead of hardcoding `table_count`/`tables_detected` to at most one
//!   whole-page table. This test pins the metadata for a real single-table
//!   image so the refactor is proven not to regress the common case (no
//!   available fixture exercises two independent tables on one page).
//! - #189: `ResultIterator::extract_all_words` now forwards
//!   `TessResultIteratorWordRecognitionLanguage` per word, surfaced as
//!   `word_language` in each `OcrElement.backend_metadata`.
//! - #175/#191: block type, justification, and crown/list-item paragraph
//!   metadata are present on every word-level `OcrElement`.

#![cfg(feature = "ocr")]

mod helpers;
use helpers::*;
use xberg::core::config::{ExtractionConfig, OcrConfig, OutputFormat};

fn tesseract_eng_config(output_format: OutputFormat) -> ExtractionConfig {
    ExtractionConfig {
        output_format,
        ocr: Some(OcrConfig {
            backend: "tesseract".to_string(),
            language: vec!["eng".to_string()],
            ..Default::default()
        }),
        force_ocr: false,
        ..Default::default()
    }
}

/// #177: table-region clustering must still report exactly one table (not
/// zero, and not a hardcoded miscount) for a real single-table image, with
/// row/column counts matching the actual reconstructed grid.
#[test]
fn single_table_image_reports_consistent_table_metadata() {
    if skip_if_missing("images/simple_table.png") {
        return;
    }
    let file_path = get_test_file_path("images/simple_table.png");
    let result = extract_uri_document_blocking(&file_path, None, &tesseract_eng_config(OutputFormat::Markdown))
        .expect("should extract simple_table.png with OCR table detection");

    let additional = &result.metadata.additional;

    assert_eq!(
        additional.get("table_count").and_then(|v| v.as_u64()),
        Some(1),
        "expected exactly one detected table region, got: {:?}",
        additional.get("table_count")
    );
    assert_eq!(
        additional.get("tables_detected").and_then(|v| v.as_u64()),
        Some(1),
        "table_count and tables_detected must agree"
    );
    assert_eq!(
        additional.get("table_rows").and_then(|v| v.as_u64()),
        Some(5),
        "expected the reconstructed table to keep its 5 rows"
    );
    assert_eq!(
        additional.get("table_cols").and_then(|v| v.as_u64()),
        Some(4),
        "expected the reconstructed table to keep its 4 columns"
    );
}

/// #189: every word-level `OcrElement` from a single-language `eng` OCR run
/// must report `word_language: "eng"` in `backend_metadata`, not omit it.
#[test]
fn word_language_is_forwarded_per_ocr_element() {
    if skip_if_missing("images/test_hello_world.png") {
        return;
    }
    let file_path = get_test_file_path("images/test_hello_world.png");
    let result = extract_uri_document_blocking(&file_path, None, &tesseract_eng_config(OutputFormat::Plain))
        .expect("should extract test_hello_world.png with OCR");

    let elements = result.ocr_elements.expect("OCR should produce word-level elements");
    assert_eq!(elements.len(), 2, "expected exactly the two words 'Hello' and 'World'");

    for element in &elements {
        assert_eq!(
            element.backend_metadata.get("word_language"),
            Some(&serde_json::json!("eng")),
            "word {:?} should report its recognition language",
            element.text
        );
        assert_eq!(
            element.backend_metadata.get("block_type"),
            Some(&serde_json::json!("PT_FLOWING_TEXT")),
            "word {:?} should carry its Tesseract block type (#175)",
            element.text
        );
        assert_eq!(
            element.backend_metadata.get("is_crown"),
            Some(&serde_json::json!(false)),
            "word {:?} should carry paragraph crown metadata (#191)",
            element.text
        );
    }

    let texts: Vec<&str> = elements.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, vec!["Hello", "World"]);
}
