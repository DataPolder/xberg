//! Integration tests for annotation editing functionality.
//!
//! Tests the complete annotation workflow:
//! - Reading annotations from PDFs
//! - Adding new annotations via Editor API
//! - Modifying existing annotations
//! - Round-trip preservation of annotation properties

use xberg_native_pdf::annotation_types::AnnotationSubtype;
use xberg_native_pdf::editor::{AnnotationWrapper, DocumentEditor};
use xberg_native_pdf::geometry::Rect;
use xberg_native_pdf::writer::{DocumentBuilder, LinkAnnotation, PageSize, TextAnnotation};

mod common;

/// Helper to create a simple test PDF with text.
fn create_simple_pdf() -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    {
        let page = builder.page(PageSize::Letter);
        page.at(72.0, 720.0).text("Hello World").done();
    }
    builder.build().expect("Failed to build PDF")
}

/// Helper to create a test PDF with a link annotation.
fn create_pdf_with_link() -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    {
        let page = builder.page(PageSize::Letter);
        page.at(72.0, 720.0)
            .text("Click here")
            .link_url("https://example.com")
            .done();
    }
    builder.build().expect("Failed to build PDF")
}

/// Helper to create a test PDF with highlighted text.
fn create_pdf_with_highlight() -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    {
        let page = builder.page(PageSize::Letter);
        page.at(72.0, 720.0)
            .text("Highlighted text")
            .highlight((1.0, 1.0, 0.0))
            .done();
    }
    builder.build().expect("Failed to build PDF")
}

/// Test that we can read annotations from a page.
#[test]
fn test_read_page_annotations() {
    let bytes = create_pdf_with_link();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_read_annotations.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let page = editor.get_page(0).expect("Failed to get page");

    assert!(
        page.annotation_count() >= 1,
        "Expected at least 1 annotation, found {}",
        page.annotation_count()
    );

    let _ = std::fs::remove_file(&temp_path);
}

/// Test adding a new link annotation via Editor API.
#[test]
fn test_add_link_annotation() {
    let bytes = create_simple_pdf();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_add_link.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let mut page = editor.get_page(0).expect("Failed to get page");

    let initial_count = page.annotation_count();

    let link = LinkAnnotation::uri(Rect::new(72.0, 700.0, 100.0, 20.0), "https://rust-lang.org");
    let annot_id = page.add_annotation(link);

    assert_eq!(page.annotation_count(), initial_count + 1);

    let found = page.find_annotation(annot_id);
    assert!(found.is_some(), "Should find annotation by ID");
    assert_eq!(found.unwrap().subtype(), AnnotationSubtype::Link);

    editor.save_page(page).expect("Failed to save page");

    let _ = std::fs::remove_file(&temp_path);
}

/// Test adding a text (sticky note) annotation.
#[test]
fn test_add_sticky_note_annotation() {
    let bytes = create_simple_pdf();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_sticky_note.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let mut page = editor.get_page(0).expect("Failed to get page");

    let note = TextAnnotation::new(Rect::new(100.0, 700.0, 24.0, 24.0), "Please review this section");
    let annot_id = page.add_annotation(note);

    let annot = page.find_annotation(annot_id).expect("Should find annotation");
    assert_eq!(annot.subtype(), AnnotationSubtype::Text);

    editor.save_page(page).expect("Failed to save page");
    let _ = std::fs::remove_file(&temp_path);
}

/// Test removing an annotation.
#[test]
fn test_remove_annotation() {
    let bytes = create_pdf_with_link();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_remove_annotation.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let mut page = editor.get_page(0).expect("Failed to get page");

    let initial_count = page.annotation_count();
    assert!(initial_count >= 1, "Need at least 1 annotation to test removal");

    let removed = page.remove_annotation(0);
    assert!(removed.is_some(), "Should return removed annotation");

    assert_eq!(page.annotation_count(), initial_count - 1);

    editor.save_page(page).expect("Failed to save page");
    let _ = std::fs::remove_file(&temp_path);
}

/// Test finding annotations by type.
#[test]
fn test_find_annotations_by_type() {
    let bytes = create_pdf_with_link();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_find_by_type.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let page = editor.get_page(0).expect("Failed to get page");

    let links = page.find_annotations_by_type(AnnotationSubtype::Link);
    assert!(!links.is_empty(), "Should find at least 1 link annotation");

    let _ = std::fs::remove_file(&temp_path);
}

/// Test finding annotations in a region.
#[test]
fn test_find_annotations_in_region() {
    let bytes = create_pdf_with_link();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_find_in_region.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let page = editor.get_page(0).expect("Failed to get page");

    let top_region = Rect::new(0.0, 600.0, 612.0, 200.0);
    let top_annotations = page.find_annotations_in_region(top_region);

    assert!(!top_annotations.is_empty(), "Should find annotations in top region");

    let _ = std::fs::remove_file(&temp_path);
}

/// Test that annotations track modification state.
#[test]
fn test_annotation_modification_tracking() {
    let bytes = create_simple_pdf();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_modification_tracking.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let mut page = editor.get_page(0).expect("Failed to get page");

    assert!(!page.has_annotations_modified());

    let note = TextAnnotation::new(Rect::new(100.0, 700.0, 24.0, 24.0), "New note");
    page.add_annotation(note);

    assert!(page.has_annotations_modified());

    let _ = std::fs::remove_file(&temp_path);
}

/// Test round-trip preservation of raw dictionary.
#[test]
fn test_raw_dict_preservation() {
    let bytes = create_pdf_with_link();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_raw_dict.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let page = editor.get_page(0).expect("Failed to get page");

    for annot in page.annotations() {
        if !annot.is_new() {
            // Existing annotations should have raw_dict available
            // (for round-trip preservation) ~keep
            let raw = annot.raw_dict();
            assert!(raw.is_some(), "Annotations from PDF should have raw_dict");
        }
    }

    let _ = std::fs::remove_file(&temp_path);
}

/// Test AnnotationWrapper subtype detection for writer annotations.
#[test]
fn test_annotation_wrapper_subtype_detection() {
    let link = LinkAnnotation::uri(Rect::new(0.0, 0.0, 100.0, 20.0), "https://example.com");
    let wrapper = AnnotationWrapper::from_write(link);
    assert_eq!(wrapper.subtype(), AnnotationSubtype::Link);

    let note = TextAnnotation::new(Rect::new(0.0, 0.0, 24.0, 24.0), "Comment");
    let wrapper = AnnotationWrapper::from_write(note);
    assert_eq!(wrapper.subtype(), AnnotationSubtype::Text);

    assert!(wrapper.is_new());
    assert!(wrapper.is_modified());
}

/// Test that PdfPage exposes annotations correctly.
#[test]
fn test_pdf_page_annotation_accessors() {
    let bytes = create_simple_pdf();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_accessors.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let mut page = editor.get_page(0).expect("Failed to get page");

    page.add_annotation(TextAnnotation::new(Rect::new(100.0, 700.0, 24.0, 24.0), "Note 1"));
    page.add_annotation(TextAnnotation::new(Rect::new(200.0, 700.0, 24.0, 24.0), "Note 2"));

    let annotations = page.annotations();
    assert!(annotations.len() >= 2);

    let first = page.annotation(0);
    assert!(first.is_some());

    let first_mut = page.annotation_mut(0);
    assert!(first_mut.is_some());

    let count = page.annotation_count();
    assert!(count >= 2);

    let _ = std::fs::remove_file(&temp_path);
}

/// Test remove annotation by ID.
#[test]
fn test_remove_annotation_by_id() {
    let bytes = create_simple_pdf();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_remove_by_id.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let mut page = editor.get_page(0).expect("Failed to get page");

    let note = TextAnnotation::new(Rect::new(100.0, 700.0, 24.0, 24.0), "To be removed");
    let annot_id = page.add_annotation(note);

    let count_after_add = page.annotation_count();

    let removed = page.remove_annotation_by_id(annot_id);
    assert!(removed.is_some(), "Should find and remove annotation by ID");

    assert_eq!(page.annotation_count(), count_after_add - 1);

    assert!(page.find_annotation(annot_id).is_none());

    let _ = std::fs::remove_file(&temp_path);
}

/// Test highlight annotation integration.
#[test]
fn test_highlight_annotation() {
    let bytes = create_pdf_with_highlight();

    let (_temp_dir, temp_path) = common::write_temp_pdf(&bytes, "test_highlight.pdf");

    let mut editor = DocumentEditor::open(&temp_path).expect("Failed to open PDF");
    let page = editor.get_page(0).expect("Failed to get page");

    let highlights = page.find_annotations_by_type(AnnotationSubtype::Highlight);
    assert!(!highlights.is_empty(), "Should find at least 1 highlight annotation");

    let _ = std::fs::remove_file(&temp_path);
}
