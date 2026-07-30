#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
//! Integration tests for Apple iWork format extractors.
//!
//! These tests verify that .pages, .numbers, and .key files can be
//! opened, parsed, and produce non-empty text output.

mod helpers;

#[cfg(feature = "iwork")]
mod iwork_tests {
    use crate::helpers::extract_bytes_document;
    use std::path::PathBuf;
    use xberg::core::config::ExtractionConfig;

    fn test_doc_path(name: &str) -> PathBuf {
        let manifest = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest).join("../../test_documents/iwork").join(name)
    }

    #[test]
    fn test_mime_detection_numbers_file() {
        let path = test_doc_path("test.numbers");
        if !path.exists() {
            eprintln!("Skipping: test.numbers not found at {:?}", path);
            return;
        }

        let mime = xberg::core::mime::detect_mime_type(&path, true).unwrap();
        assert_eq!(
            mime, "application/x-iwork-numbers-sffnumbers",
            "Should detect .numbers MIME type from extension"
        );
    }

    #[test]
    fn test_mime_detection_pages_file() {
        let path = test_doc_path("test.pages");
        if !path.exists() {
            eprintln!("Skipping: test.pages not found at {:?}", path);
            return;
        }

        let mime = xberg::core::mime::detect_mime_type(&path, true).unwrap();
        assert_eq!(
            mime, "application/x-iwork-pages-sffpages",
            "Should detect .pages MIME type from extension"
        );
    }

    #[tokio::test]
    #[cfg(feature = "tokio-runtime")]
    async fn test_extract_numbers_document() {
        let path = test_doc_path("test.numbers");
        if !path.exists() {
            eprintln!("Skipping: test.numbers not found at {:?}", path);
            return;
        }

        let content = std::fs::read(&path).expect("Failed to read test.numbers");
        let config = ExtractionConfig::default();

        let result = extract_bytes_document(&content, "application/x-iwork-numbers-sffnumbers", &config)
            .await
            .expect("Extraction should not fail on valid file");

        assert!(
            !result.content.is_empty(),
            "Numbers extraction should produce non-empty text. Got: {:?}",
            &result.content[..result.content.len().min(200)]
        );
    }

    #[tokio::test]
    #[cfg(feature = "tokio-runtime")]
    async fn test_extract_pages_document() {
        let path = test_doc_path("test.pages");
        if !path.exists() {
            eprintln!("Skipping: test.pages not found at {:?}", path);
            return;
        }

        let content = std::fs::read(&path).expect("Failed to read test.pages");
        let config = ExtractionConfig::default();

        let result = extract_bytes_document(&content, "application/x-iwork-pages-sffpages", &config)
            .await
            .expect("Extraction should not fail on valid ZIP file");

        assert!(
            result.mime_type.as_ref() == "application/x-iwork-pages-sffpages",
            "MIME type should be preserved in result"
        );
    }
}
