//! PDF/UA accessibility tagging: `/Alt` on figures, `/Artifact` on
//! decorative images.
//!
//! This replaces a test that previously built its fixture through
//! `crate::ffi::pdf_document_builder_*`, the C FFI builder API. That module
//! (`src/ffi.rs`) has been removed from this fork, so the fixture here is
//! built directly with `xberg_native_pdf::writer::DocumentBuilder` /
//! `DocumentMetadata`, which is what the removed FFI builder wrapped.

use xberg_native_pdf::geometry::Rect;
use xberg_native_pdf::writer::{DocumentBuilder, DocumentMetadata};

const JPEG_FIXTURE: &str = "tests/fixtures/adobe_cmyk_10x11_white.jpg";

/// `image_from_bytes_with_alt` on a `tagged_pdf_ua1()` document must write
/// the alt text into the structure tree (`/Alt`) and emit the PDF/UA-1
/// catalog wiring (`/MarkInfo`, `/StructTreeRoot`).
#[test]
fn image_with_alt_writes_alt_into_structure_tree() {
    let jpeg_data = std::fs::read(JPEG_FIXTURE).expect("JPEG fixture");

    let mut builder = DocumentBuilder::new();
    builder = builder.metadata(DocumentMetadata::new().tagged_pdf_ua1().language("en-US"));
    let page = builder.letter_page().font("Helvetica", 12.0).at(72.0, 720.0);
    page.image_from_bytes_with_alt(
        &jpeg_data,
        Rect::new(72.0, 600.0, 100.0, 100.0),
        "A white JPEG test image",
    )
    .expect("image_from_bytes_with_alt")
    .done();

    let bytes = builder.build().expect("build");
    let content = String::from_utf8_lossy(&bytes);

    assert!(
        content.contains("/Alt"),
        "/Alt not found in PDF output — image alt text was not written to structure tree"
    );
    assert!(
        content.contains("A white JPEG test image"),
        "alt text string itself missing from PDF output"
    );
    assert!(content.contains("/MarkInfo"), "missing /MarkInfo");
    assert!(content.contains("/StructTreeRoot"), "missing /StructTreeRoot");
}

/// `image_from_bytes_as_artifact` must mark the image `/Artifact` and must
/// NOT attach `/Alt` text (assistive technology ignores artifacts).
#[test]
fn image_artifact_marks_decorative_image_as_artifact() {
    let jpeg_data = std::fs::read(JPEG_FIXTURE).expect("JPEG fixture");

    let mut builder = DocumentBuilder::new();
    builder = builder.metadata(DocumentMetadata::new().tagged_pdf_ua1().language("en-US"));
    let page = builder.letter_page().font("Helvetica", 12.0);
    page.image_from_bytes_as_artifact(&jpeg_data, Rect::new(72.0, 600.0, 50.0, 50.0))
        .expect("image_from_bytes_as_artifact")
        .done();

    let bytes = builder.build().expect("build");
    let content = String::from_utf8_lossy(&bytes);

    assert!(
        content.contains("/Artifact"),
        "/Artifact not found — decorative image not marked as artifact"
    );
    assert!(
        !content.contains("/Alt"),
        "decorative artifact image must not carry /Alt text"
    );
}

/// Sanity check that the assertions above are non-vacuous: without
/// `tagged_pdf_ua1()` on the document, none of the accessibility markers
/// should be present at all, even for the same image call.
#[test]
fn untagged_document_has_no_accessibility_markers() {
    let jpeg_data = std::fs::read(JPEG_FIXTURE).expect("JPEG fixture");

    let mut builder = DocumentBuilder::new();
    let page = builder.letter_page().font("Helvetica", 12.0);
    page.image_from_bytes_with_alt(&jpeg_data, Rect::new(72.0, 600.0, 100.0, 100.0), "ignored")
        .expect("image_from_bytes_with_alt")
        .done();

    let bytes = builder.build().expect("build");
    let content = String::from_utf8_lossy(&bytes);

    assert!(
        !content.contains("/MarkInfo"),
        "untagged PDF must not contain /MarkInfo"
    );
    assert!(
        !content.contains("/StructTreeRoot"),
        "untagged PDF must not contain /StructTreeRoot"
    );
}
