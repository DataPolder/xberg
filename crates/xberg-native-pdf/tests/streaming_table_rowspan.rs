//! `StreamingTable` rowspan handling: `max_rowspan` plus `span_cell` must
//! produce a valid PDF whose table content survives a re-open + text
//! extraction round-trip.
//!
//! This replaces a test that previously built its fixture through
//! `crate::ffi::pdf_document_builder_*`, the C FFI builder API. That module
//! (`src/ffi.rs`) has been removed from this fork, so the fixture here is
//! built directly with `xberg_native_pdf::writer::DocumentBuilder` and
//! `StreamingTable`/`StreamingTableConfig`/`StreamingColumn`, which is what
//! the removed FFI builder's `streaming_table_begin_v2`/`push_row_v2` wrapped.

use xberg_native_pdf::document::PdfDocument;
use xberg_native_pdf::writer::{DocumentBuilder, StreamingColumn, StreamingTableConfig};

fn build_table_pdf() -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    let page = builder.letter_page().font("Helvetica", 10.0).at(72.0, 720.0);

    let mut t = page.streaming_table(
        StreamingTableConfig::new()
            .column(StreamingColumn::new("Category").width_pt(100.0))
            .column(StreamingColumn::new("Item").width_pt(150.0))
            .column(StreamingColumn::new("Notes").width_pt(150.0))
            .repeat_header(true)
            .mode_fixed()
            .max_rowspan(2),
    );

    t.push_row(|r| {
        r.span_cell("Fruits", 2);
        r.cell("Apple");
        r.cell("Red");
    })
    .expect("push_row row1");

    t.push_row(|r| {
        r.cell("");
        r.cell("Banana");
        r.cell("Yellow");
    })
    .expect("push_row row2");

    t.push_row(|r| {
        r.cell("Vegetables");
        r.cell("Carrot");
        r.cell("Orange");
    })
    .expect("push_row row3");

    t.finish().done();

    builder.build().expect("build")
}

#[test]
fn streaming_table_rowspan_produces_valid_pdf_and_survives_round_trip() {
    let bytes = build_table_pdf();
    assert!(bytes.starts_with(b"%PDF-"), "output must be a valid PDF");

    let doc = PdfDocument::from_bytes(bytes).expect("re-open failed");
    let extracted = doc.extract_text(0).expect("extract_text failed");

    assert!(
        extracted.contains("Fruits") || extracted.contains("Apple"),
        "table content 'Fruits/Apple' not found in extracted text: {extracted:.200}"
    );
    assert!(
        extracted.contains("Banana"),
        "rowspan-continuation row content 'Banana' not found in extracted text: {extracted:.200}"
    );
    assert!(
        extracted.contains("Carrot"),
        "table content 'Carrot' not found in extracted text: {extracted:.200}"
    );
}

/// Sanity check that the round-trip assertions are non-vacuous: text that
/// was never pushed into the table must not appear in the extracted output.
#[test]
fn streaming_table_rowspan_extracted_text_excludes_unrelated_content() {
    let bytes = build_table_pdf();
    let doc = PdfDocument::from_bytes(bytes).expect("re-open failed");
    let extracted = doc.extract_text(0).expect("extract_text failed");

    assert!(
        !extracted.contains("Elephant"),
        "extracted text unexpectedly contained content that was never in the table: {extracted:.200}"
    );
}
