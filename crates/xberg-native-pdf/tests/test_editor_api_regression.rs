//! Comprehensive regression tests for the DocumentEditor API.
//!
//! Tests are organized into groups:
//!   1. overlay: add_text / add_path preserves original content
//!   2. select_pages-first ordering: modification after select_pages uses correct page
//!   3. erase_region combined with select_pages
//!   4. set_page_rotation combined with select_pages
//!   5. set_page_media_box / set_page_crop_box combined with select_pages
//!   6. Multiple pages edited in one document
//!   7. add_image / add_path overlay on existing PDFs
//!   8. Form-field-bearing PDF round-trips: a DocumentEditor no-op save or
//!      text overlay must not disturb an AcroForm built by DocumentBuilder.

use xberg_native_pdf::document::PdfDocument;
use xberg_native_pdf::editor::DocumentEditor;
use xberg_native_pdf::elements::{FontSpec, TextContent, TextStyle};
use xberg_native_pdf::geometry::Rect;
use xberg_native_pdf::writer::{DocumentBuilder, PageSize};

/// Build a minimal single-page PDF containing text and a filled grey rectangle.
fn single_page_pdf_with_content() -> Vec<u8> {
    let mut pdf = b"%PDF-1.7\n".to_vec();

    let off_catalog = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let off_pages = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let off_page = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n\
        << /Type /Page /Parent 2 0 R \
           /MediaBox [0 0 612 792] \
           /Contents 4 0 R \
           /Resources << /Font << /F1 5 0 R >> >> >>\n\
        endobj\n",
    );

    let content = b"0.8 g\n100 600 200 100 re f\n0 g\nBT /F1 14 Tf 110 640 Td (Original text) Tj ET";
    let off_content = pdf.len();
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off_font = pdf.len();
    pdf.extend_from_slice(
        b"5 0 obj\n\
        << /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
           /Encoding /WinAnsiEncoding >>\n\
        endobj\n",
    );

    let xref_pos = pdf.len();
    let offsets = [0usize, off_catalog, off_pages, off_page, off_content, off_font];
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    pdf.extend_from_slice(format!("{:010} 65535 f\r\n", 0).as_bytes());
    for &off in &offsets[1..] {
        pdf.extend_from_slice(format!("{:010} 00000 n\r\n", off).as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_pos
        )
        .as_bytes(),
    );
    pdf
}

/// Build an N-page document using DocumentBuilder.  Each page has a unique text label.
fn multi_page_pdf(page_labels: &[&str]) -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    for label in page_labels {
        let p = builder.page(PageSize::Letter);
        p.at(72.0, 720.0).text(label).done();
    }
    builder.build().expect("build multi-page PDF")
}

/// Build an N-page document, each page carrying a label and an AcroForm
/// text field, using `DocumentBuilder`. Field names are unique per page
/// (`field_0`, `field_1`, ...) so a page-selection test can tell which
/// field survived.
fn form_pdf(page_labels: &[&str]) -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    for (i, label) in page_labels.iter().enumerate() {
        builder
            .page(PageSize::Letter)
            .at(72.0, 720.0)
            .text(label)
            .text_field(
                format!("field_{i}"),
                72.0,
                650.0,
                200.0,
                20.0,
                Some(format!("value-{i}")),
            )
            .done();
    }
    builder.build().expect("build form PDF")
}

fn add_text_overlay(page: &mut xberg_native_pdf::editor::dom::PdfPage, text: &str) {
    let cx = page.width / 2.0;
    let cy = page.height / 2.0;
    let font_size = 18.0;
    let approx_width = text.len() as f32 * font_size * 0.5;
    let bbox = Rect::new(cx - approx_width / 2.0, cy - font_size / 2.0, approx_width, font_size);
    page.add_text(TextContent::new(
        text,
        bbox,
        FontSpec::helvetica(font_size),
        TextStyle::new(),
    ));
}

#[test]
fn overlay_add_text_preserves_original_text_and_graphics() {
    let source = single_page_pdf_with_content();
    let mut editor = DocumentEditor::from_bytes(source).expect("open PDF");
    let mut page = editor.get_page(0).expect("get_page");
    add_text_overlay(&mut page, "overlay text");
    editor.save_page(page).expect("save_page");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(4).any(|w| w == b"re f"),
        "graphics operator 're f' lost after add_text"
    );

    let doc = PdfDocument::from_bytes(bytes.clone()).expect("reopen");
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(
        all.contains("Original text"),
        "original text lost after add_text; got: {all:?}"
    );
    let overlay_present = all.contains("overlay text") || bytes.windows(12).any(|w| w == b"overlay text");
    assert!(overlay_present, "overlay text not found in output; got: {all:?}");
}

#[test]
fn overlay_add_path_preserves_original_content() {
    use xberg_native_pdf::elements::{PathContent, PathOperation};

    let source = single_page_pdf_with_content();
    let mut editor = DocumentEditor::from_bytes(source).expect("open PDF");
    let mut page = editor.get_page(0).expect("get_page");

    let path = PathContent::from_operations(vec![
        PathOperation::MoveTo(50.0, 50.0),
        PathOperation::LineTo(150.0, 150.0),
    ]);
    page.add_path(path);
    editor.save_page(page).expect("save_page");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(4).any(|w| w == b"re f"),
        "original rectangle fill lost after add_path"
    );

    let doc = PdfDocument::from_bytes(bytes).expect("reopen");
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(
        all.contains("Original text"),
        "original text lost after add_path; got: {all:?}"
    );
}

/// add_text overlay correctly applied when select_pages is called BEFORE get_page/save_page.
#[test]
fn overlay_add_text_after_select_pages() {
    let source = multi_page_pdf(&["Page zero", "Page one", "Page two"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[1]).expect("select_pages");

    let mut page = editor.get_page(0).expect("get_page after select_pages");
    add_text_overlay(&mut page, "post-select overlay");
    editor.save_page(page).expect("save_page");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    let doc = PdfDocument::from_bytes(bytes.clone()).expect("reopen");
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");

    assert!(
        all.contains("Page one"),
        "selected page original text lost; got: {all:?}"
    );
    let overlay_present = all.contains("post-select overlay")
        || bytes
            .windows(20)
            .any(|w| w == b"post-select overlay\n".get(..w.len()).unwrap_or(&[]));
    let overlay_raw = bytes.windows(19).any(|w| w == b"post-select overlay");
    assert!(
        overlay_present || overlay_raw,
        "overlay not found after select_pages-first add_text; got: {all:?}"
    );
}

/// Variant: select_pages keeps page 2 (index 2), add_text should land on it.
#[test]
fn overlay_add_text_after_select_pages_last_page() {
    let source = multi_page_pdf(&["First", "Second", "Third"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[2]).expect("select_pages");

    let mut page = editor.get_page(0).expect("get_page");
    add_text_overlay(&mut page, "third-page-overlay");
    editor.save_page(page).expect("save_page");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    let doc = PdfDocument::from_bytes(bytes.clone()).expect("reopen");
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");

    assert!(
        all.contains("Third"),
        "original text of selected page not found; got: {all:?}"
    );
    let has_overlay = all.contains("third-page-overlay") || bytes.windows(18).any(|w| w == b"third-page-overlay");
    assert!(has_overlay, "overlay not found; got: {all:?}");
}

/// erase_region called AFTER select_pages should affect the correct (selected) page.
#[test]
fn erase_region_after_select_pages() {
    let source = multi_page_pdf(&["First page text", "Second page text", "Third page text"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[1]).expect("select_pages");
    editor.erase_region(0, [0.0, 0.0, 612.0, 792.0]).expect("erase_region");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(9).any(|w| w == b"1 1 1 rg\n"),
        "erase overlay not applied after select_pages-first erase_region"
    );
}

/// erase_region called BEFORE select_pages (traditional order) must still work.
#[test]
fn erase_region_before_select_pages() {
    let source = multi_page_pdf(&["Alpha", "Beta", "Gamma"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.erase_region(1, [0.0, 0.0, 612.0, 792.0]).expect("erase_region");
    editor.select_pages(&[1]).expect("select_pages");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(9).any(|w| w == b"1 1 1 rg\n"),
        "erase overlay lost when select_pages called after erase_region"
    );
}

/// Rotation set AFTER select_pages must be applied to the correct page.
#[test]
fn set_page_rotation_after_select_pages() {
    let source = multi_page_pdf(&["P0", "P1", "P2"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[1]).expect("select_pages");
    editor.set_page_rotation(0, 90).expect("set_page_rotation");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(10).any(|w| w == b"/Rotate 90"),
        "/Rotate 90 not found in output after select_pages-first rotation"
    );
}

/// Rotation set BEFORE select_pages (traditional order) must survive.
#[test]
fn set_page_rotation_before_select_pages() {
    let source = multi_page_pdf(&["P0", "P1", "P2"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.set_page_rotation(1, 180).expect("set_page_rotation");
    editor.select_pages(&[1]).expect("select_pages");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(11).any(|w| w == b"/Rotate 180"),
        "/Rotate 180 lost when select_pages called after set_page_rotation"
    );
}

/// get_page_rotation must reflect set_page_rotation after select_pages.
#[test]
fn get_page_rotation_consistent_after_select_pages() {
    let source = multi_page_pdf(&["P0", "P1", "P2"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[2]).expect("select_pages");
    editor.set_page_rotation(0, 270).expect("set_page_rotation");

    let rotation = editor.get_page_rotation(0).expect("get_page_rotation");
    assert_eq!(
        rotation, 270,
        "get_page_rotation did not reflect set_page_rotation after select_pages"
    );
}

#[test]
fn set_page_media_box_after_select_pages() {
    let source = multi_page_pdf(&["P0", "P1", "P2"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[1]).expect("select_pages");
    editor
        .set_page_media_box(0, [0.0, 0.0, 400.0, 600.0])
        .expect("set_page_media_box");

    let mb = editor.get_page_media_box(0).expect("get_page_media_box");
    assert_eq!(
        mb,
        [0.0, 0.0, 400.0, 600.0],
        "media_box not reflected by getter after select_pages"
    );

    let bytes = editor.save_to_bytes().expect("save_to_bytes");
    assert!(
        bytes.windows(8).any(|w| w == b"MediaBox"),
        "/MediaBox not written to output"
    );
}

#[test]
fn set_page_crop_box_after_select_pages() {
    let source = multi_page_pdf(&["P0", "P1", "P2"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor.select_pages(&[0]).expect("select_pages");
    editor
        .set_page_crop_box(0, [10.0, 10.0, 590.0, 780.0])
        .expect("set_page_crop_box");

    let cb = editor.get_page_crop_box(0).expect("get_page_crop_box");
    assert_eq!(
        cb,
        Some([10.0, 10.0, 590.0, 780.0]),
        "crop_box not reflected by getter after select_pages"
    );
}

/// Edit three pages independently in one editor session.
#[test]
fn multiple_pages_with_overlays() {
    let source = multi_page_pdf(&["Alpha", "Beta", "Gamma"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    for i in 0..3 {
        let mut page = editor.get_page(i).expect("get_page");
        add_text_overlay(&mut page, &format!("overlay-{i}"));
        editor.save_page(page).expect("save_page");
    }

    let bytes = editor.save_to_bytes().expect("save_to_bytes");
    let doc = PdfDocument::from_bytes(bytes.clone()).expect("reopen");

    for i in 0..3 {
        let spans = doc.extract_spans(i).expect("extract_spans");
        let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        let label = ["Alpha", "Beta", "Gamma"][i];
        assert!(
            all.contains(label),
            "original text '{label}' lost on page {i}; got: {all:?}"
        );
    }

    for i in 0..3 {
        let tag = format!("overlay-{i}");
        assert!(
            bytes.windows(tag.len()).any(|w| w == tag.as_bytes()),
            "overlay '{tag}' not found in output bytes"
        );
    }
}

/// Same page saved twice: second save must not corrupt the page.
#[test]
fn save_page_twice_second_wins() {
    let source = single_page_pdf_with_content();
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    {
        let mut page = editor.get_page(0).expect("get_page 1");
        add_text_overlay(&mut page, "first overlay");
        editor.save_page(page).expect("save_page 1");
    }
    {
        let mut page = editor.get_page(0).expect("get_page 2");
        add_text_overlay(&mut page, "second overlay");
        editor.save_page(page).expect("save_page 2");
    }

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(4).any(|w| w == b"re f"),
        "original rectangle lost after two consecutive save_page calls"
    );

    let doc = PdfDocument::from_bytes(bytes.clone()).expect("reopen");
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(
        all.contains("Original text"),
        "original text lost after two save_page; got: {all:?}"
    );
}

#[test]
fn erase_then_add_text_on_existing_page() {
    let source = single_page_pdf_with_content();
    let mut editor = DocumentEditor::from_bytes(source).expect("open");

    editor
        .erase_region(0, [100.0, 600.0, 200.0, 100.0])
        .expect("erase_region");

    let mut page = editor.get_page(0).expect("get_page");
    add_text_overlay(&mut page, "replacement text");
    editor.save_page(page).expect("save_page");

    let bytes = editor.save_to_bytes().expect("save_to_bytes");

    assert!(
        bytes.windows(9).any(|w| w == b"1 1 1 rg\n"),
        "erase overlay not found in output"
    );
    let has_text = bytes.windows(16).any(|w| w == b"replacement text");
    assert!(has_text, "replacement text not found in output");
}

/// A no-op DocumentEditor round-trip on a form PDF must keep the AcroForm
/// dictionary and its field intact.
#[test]
fn form_pdf_noop_roundtrip_preserves_acroform() {
    let source = form_pdf(&["Wage statement"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open form PDF");

    let output = editor.save_to_bytes().expect("save form PDF");

    assert!(
        output.windows(8).any(|w| w == b"AcroForm"),
        "AcroForm dictionary lost on no-op round-trip"
    );
    assert!(
        output.windows(8).any(|w| w == b"field_0)"),
        "field name lost on no-op round-trip"
    );

    let doc = PdfDocument::from_bytes(output).expect("re-open form PDF output");
    assert_eq!(
        doc.page_count().expect("page count"),
        1,
        "page count changed on no-op round-trip"
    );
}

/// Adding a text overlay to a form PDF must not destroy the original field
/// or page text content.
#[test]
fn form_pdf_add_text_overlay_preserves_field_and_content() {
    let source = form_pdf(&["Wage statement"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open form PDF");

    let mut page = editor.get_page(0).expect("get_page 0");
    add_text_overlay(&mut page, "test annotation");
    editor.save_page(page).expect("save_page");

    let output = editor.save_to_bytes().expect("save form PDF with overlay");

    let has_annotation = output.windows(15).any(|w| w == b"test annotation");
    assert!(
        has_annotation,
        "overlay text 'test annotation' not found in output bytes"
    );
    assert!(
        output.windows(8).any(|w| w == b"AcroForm"),
        "AcroForm dictionary lost after overlay"
    );
    assert!(
        output.windows(8).any(|w| w == b"field_0)"),
        "field name lost after overlay"
    );

    let doc = PdfDocument::from_bytes(output).expect("re-open with overlay");
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(
        all.contains("Wage statement"),
        "original page text lost after overlay; got: {all:?}"
    );
}

/// select_pages on a multi-page form PDF keeps only the selected page and
/// its field, dropping the others.
#[test]
fn form_pdf_select_pages_preserves_content() {
    let source = form_pdf(&["Page zero", "Page one", "Page two"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open form PDF");
    editor.select_pages(&[1]).expect("select_pages");

    let output = editor.save_to_bytes().expect("save after select_pages");
    let doc = PdfDocument::from_bytes(output.clone()).expect("re-open after select_pages");

    assert_eq!(
        doc.page_count().expect("page count"),
        1,
        "expected exactly 1 page after select_pages"
    );
    let spans = doc.extract_spans(0).expect("extract_spans");
    let all: String = spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(
        all.contains("Page one"),
        "selected page's text lost after select_pages; got: {all:?}"
    );
    assert!(
        output.windows(8).any(|w| w == b"field_1)"),
        "selected page's field lost after select_pages"
    );
}

/// select_pages-then-add_text on a form PDF: overlay must land on the
/// selected page without disturbing its field.
#[test]
fn form_pdf_select_pages_then_add_text() {
    let source = form_pdf(&["Page zero", "Page one", "Page two"]);
    let mut editor = DocumentEditor::from_bytes(source).expect("open form PDF");

    editor.select_pages(&[1]).expect("select_pages");

    let mut page = editor.get_page(0).expect("get_page after select_pages");
    add_text_overlay(&mut page, "post-select-form-overlay");
    editor.save_page(page).expect("save_page");

    let output = editor.save_to_bytes().expect("save_to_bytes");

    let has_overlay = output.windows(24).any(|w| w == b"post-select-form-overlay");
    assert!(has_overlay, "overlay text not found after select_pages on form PDF");
    assert!(
        output.windows(8).any(|w| w == b"field_1)"),
        "selected page's field lost after select_pages"
    );
}
