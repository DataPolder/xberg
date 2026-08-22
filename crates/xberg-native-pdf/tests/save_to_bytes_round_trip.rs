//! `Pdf::save_to_bytes` / `Pdf::from_bytes` in-memory round-trip.
//!
//! This replaces a test that previously built its fixture through
//! `crate::ffi::pdf_document_builder_*`, the C FFI builder API. That module
//! (`src/ffi.rs`) has been removed from this fork, so the fixture here is
//! built directly with the high-level `xberg_native_pdf::api::Pdf` Rust API instead.

use xberg_native_pdf::api::Pdf;

/// A PDF built entirely in memory (no filesystem) must serialize via
/// `save_to_bytes`, and the resulting bytes must re-open via `from_bytes`
/// with the original text still extractable.
#[test]
fn save_to_bytes_round_trip_preserves_text() {
    let mut pdf = Pdf::from_text("In-memory round-trip content").expect("build PDF");

    let bytes = pdf.save_to_bytes().expect("save_to_bytes");
    assert!(bytes.starts_with(b"%PDF-"), "save_to_bytes must produce a valid PDF");
    assert!(!bytes.is_empty(), "save_to_bytes produced 0 bytes");

    let mut reopened = Pdf::from_bytes(bytes).expect("from_bytes on round-tripped PDF");
    let extracted = reopened.to_text(0).expect("extract text from reopened PDF");

    assert!(
        extracted.contains("In-memory"),
        "extracted text missing 'In-memory': {extracted:.200}"
    );
}

/// Sanity check that the round trip genuinely exercises text content and is
/// not vacuously true: a PDF built with different text must not extract to
/// contain the original marker string.
#[test]
fn save_to_bytes_round_trip_reflects_actual_content() {
    let mut pdf = Pdf::from_text("Completely different marker text").expect("build PDF");
    let bytes = pdf.save_to_bytes().expect("save_to_bytes");
    let mut reopened = Pdf::from_bytes(bytes).expect("from_bytes");
    let extracted = reopened.to_text(0).expect("extract text");

    assert!(
        !extracted.contains("In-memory"),
        "unrelated PDF unexpectedly contained the other fixture's marker text: {extracted:.200}"
    );
    assert!(
        extracted.contains("Completely different marker text"),
        "extracted text missing its own content: {extracted:.200}"
    );
}
