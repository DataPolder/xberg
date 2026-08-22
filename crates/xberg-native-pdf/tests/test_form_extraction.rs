//! Integration tests for form field extraction and checkbox text leak prevention.

use std::io::Write;
use tempfile::NamedTempFile;
use xberg_native_pdf::document::PdfDocument;
use xberg_native_pdf::extractors::forms::{FieldType, FieldValue, FormExtractor};
use xberg_native_pdf::geometry::Rect;
use xberg_native_pdf::writer::{
    CheckboxWidget, ComboBoxWidget, ListBoxWidget, PdfWriter, TextFieldWidget,
};

/// Create a test PDF with various form field types and return bytes.
fn create_form_pdf_bytes() -> Vec<u8> {
    let mut writer = PdfWriter::new();
    {
        let mut page = writer.add_page(612.0, 792.0);

        page.add_text_field(
            TextFieldWidget::new("name", Rect::new(72.0, 700.0, 200.0, 20.0))
                .with_value("John Doe")
                .required(),
        );

        page.add_text_field(
            TextFieldWidget::new("ssn", Rect::new(72.0, 670.0, 150.0, 20.0))
                .with_value("123-45-6789")
                .read_only()
                .with_max_length(11),
        );

        page.add_checkbox(
            CheckboxWidget::new("agree", Rect::new(72.0, 640.0, 15.0, 15.0)).checked(),
        );

        page.add_checkbox(CheckboxWidget::new("newsletter", Rect::new(72.0, 610.0, 15.0, 15.0)));

        page.add_combo_box(
            ComboBoxWidget::new("country", Rect::new(72.0, 580.0, 150.0, 20.0))
                .with_options(vec!["USA", "Canada", "UK"])
                .with_value("USA"),
        );

        page.add_list_box(
            ListBoxWidget::new("interests", Rect::new(72.0, 500.0, 150.0, 80.0))
                .with_options(vec!["Sports", "Music", "Art", "Technology"])
                .multi_select(),
        );
    }
    writer.finish().expect("Failed to create test PDF")
}

/// Helper to write bytes to a temp file and open as PdfDocument.
fn open_pdf_from_bytes(bytes: &[u8]) -> (NamedTempFile, PdfDocument) {
    let mut temp = NamedTempFile::new().expect("Failed to create temp file");
    temp.write_all(bytes).expect("Failed to write temp file");
    let doc = PdfDocument::open(temp.path().to_str().unwrap()).expect("Failed to open test PDF");
    (temp, doc)
}

#[test]
fn test_extract_form_fields_basic() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");

    assert!(fields.len() >= 6, "Expected at least 6 fields, got {}", fields.len());
}

#[test]
fn test_extract_text_field_value() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");

    let name_field = fields.iter().find(|f| f.full_name == "name");
    assert!(name_field.is_some(), "Should find 'name' field");
    let name_field = name_field.unwrap();

    assert_eq!(name_field.field_type, FieldType::Text);
    assert_eq!(name_field.value, FieldValue::Text("John Doe".to_string()));
}

#[test]
fn test_extract_text_field_readonly_flag() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");

    let ssn_field = fields.iter().find(|f| f.full_name == "ssn");
    assert!(ssn_field.is_some(), "Should find 'ssn' field");
    let ssn_field = ssn_field.unwrap();

    assert!(ssn_field.flags.is_some_and(|f| f & 1 != 0), "SSN field should be read-only");
}

#[test]
fn test_extract_checkbox_field() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");

    let agree_field = fields.iter().find(|f| f.full_name == "agree");
    assert!(agree_field.is_some(), "Should find 'agree' checkbox");
    let agree_field = agree_field.unwrap();

    assert_eq!(agree_field.field_type, FieldType::Button);
}

#[test]
fn test_extract_choice_field() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");

    let country_field = fields.iter().find(|f| f.full_name == "country");
    assert!(country_field.is_some(), "Should find 'country' choice field");
    let country_field = country_field.unwrap();

    assert_eq!(country_field.field_type, FieldType::Choice);
}

#[test]
fn test_extract_no_form_fields_on_plain_pdf() {
    let bytes = xberg_native_pdf::api::Pdf::from_text("No forms here")
        .unwrap()
        .into_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");
    assert!(fields.is_empty(), "Plain PDF should have no form fields");
}

#[test]
fn test_form_field_has_bounds() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");

    let with_bounds = fields.iter().filter(|f| f.bounds.is_some()).count();
    assert!(with_bounds > 0, "At least some fields should have bounding boxes");
}

#[test]
fn test_checkbox_does_not_leak_off_into_text() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    let tokens: Vec<&str> = text.split_whitespace().collect();
    assert!(
        !tokens.contains(&"Off"),
        "Extracted text should not contain checkbox 'Off' state.\nGot: {}",
        text
    );
}

#[test]
fn test_checkbox_does_not_leak_yes_into_text() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    let tokens: Vec<&str> = text.split_whitespace().collect();
    assert!(
        !tokens.contains(&"Yes"),
        "Extracted text should not contain checkbox 'Yes' state.\nGot: {}",
        text
    );
}

#[test]
fn test_checkbox_does_not_leak_zapf_dingbats() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    assert!(
        !text.contains('\u{2714}'),
        "Extracted text should not contain ZapfDingbats checkmark.\nGot: {}",
        text
    );
}

#[test]
fn test_text_field_values_may_appear_in_text() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let _text = doc.extract_text(0).expect("Failed to extract text");
    let _fields = FormExtractor::extract_fields(&doc).expect("Failed to extract fields");
}

#[test]
fn test_editor_get_form_fields() {
    let bytes = create_form_pdf_bytes();
    let mut temp = NamedTempFile::new().expect("Failed to create temp file");
    temp.write_all(&bytes).expect("Failed to write temp file");

    let mut editor = xberg_native_pdf::editor::DocumentEditor::open(temp.path().to_str().unwrap())
        .expect("Failed to open editor");

    let fields = editor.get_form_fields().expect("Failed to get form fields");
    assert!(fields.len() >= 6, "Expected at least 6 fields via editor, got {}", fields.len());
}

#[test]
fn test_editor_get_set_form_field_value() {
    let bytes = create_form_pdf_bytes();
    let mut temp = NamedTempFile::new().expect("Failed to create temp file");
    temp.write_all(&bytes).expect("Failed to write temp file");

    let mut editor = xberg_native_pdf::editor::DocumentEditor::open(temp.path().to_str().unwrap())
        .expect("Failed to open editor");

    let value = editor
        .get_form_field_value("name")
        .expect("Failed to get field value");
    assert!(value.is_some(), "Should find 'name' field value");

    use xberg_native_pdf::editor::form_fields::FormFieldValue;
    editor
        .set_form_field_value("name", FormFieldValue::Text("Jane Doe".to_string()))
        .expect("Failed to set field value");

    let updated = editor
        .get_form_field_value("name")
        .expect("Failed to get updated value");
    assert_eq!(updated, Some(FormFieldValue::Text("Jane Doe".to_string())));
}

#[test]
fn test_has_xfa_on_non_xfa_pdf() {
    let bytes = create_form_pdf_bytes();
    let (_temp, mut doc) = open_pdf_from_bytes(&bytes);

    let has_xfa =
        xberg_native_pdf::xfa::XfaExtractor::has_xfa(&mut doc).expect("Failed to check XFA");

    assert!(!has_xfa, "Writer-created form should not have XFA");
}

#[test]
fn test_has_xfa_on_plain_pdf() {
    let bytes = xberg_native_pdf::api::Pdf::from_text("No forms")
        .unwrap()
        .into_bytes();
    let (_temp, mut doc) = open_pdf_from_bytes(&bytes);

    let has_xfa =
        xberg_native_pdf::xfa::XfaExtractor::has_xfa(&mut doc).expect("Failed to check XFA");

    assert!(!has_xfa, "Plain text PDF should not have XFA");
}

#[test]
fn test_extract_text_form_fields_inline() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    assert!(
        text.contains("John Doe"),
        "Text field value 'John Doe' should appear in extracted text.\nGot: {}",
        text
    );
    assert!(
        text.contains("123-45-6789"),
        "Text field value '123-45-6789' should appear in extracted text.\nGot: {}",
        text
    );
}

#[test]
fn test_widget_spans_checkbox_checked() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    assert!(
        text.contains("[x]"),
        "Checked checkbox should render as '[x]' in extracted text.\nGot: {}",
        text
    );
}

#[test]
fn test_widget_spans_checkbox_unchecked() {
    // An UNCHECKED checkbox carries no text and must NOT emit a "[ ]" marker
    // (CORPUS-1): that synthetic noise made xberg-native-pdf diverge from pdftotext /
    // PyMuPDF on AcroForm-heavy PDFs. Only the meaningful checked state "[x]"
    // is surfaced (see test_widget_spans_checkbox_checked). ~keep
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    assert!(
        !text.contains("[ ]"),
        "Unchecked checkbox must NOT emit a '[ ]' marker (noise).\nGot: {}",
        text
    );
}

#[test]
fn test_widget_spans_choice_field() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");

    assert!(
        text.contains("USA"),
        "Choice field value 'USA' should appear in extracted text.\nGot: {}",
        text
    );
}

#[test]
fn test_parse_font_size_from_da() {
    let bytes = create_form_pdf_bytes();
    let (_temp, doc) = open_pdf_from_bytes(&bytes);

    let text = doc.extract_text(0).expect("Failed to extract text");
    assert!(!text.is_empty(), "Extracted text should not be empty");
}

#[test]
fn test_save_incremental_persists_text_value() {
    use xberg_native_pdf::editor::form_fields::FormFieldValue;
    use xberg_native_pdf::editor::{DocumentEditor, EditableDocument, SaveOptions};

    let bytes = create_form_pdf_bytes();
    let mut temp = NamedTempFile::new().expect("create temp");
    temp.write_all(&bytes).expect("write temp");

    let mut editor = DocumentEditor::open(temp.path().to_str().unwrap()).expect("open editor");
    editor
        .set_form_field_value("name", FormFieldValue::Text("Jane Doe".into()))
        .expect("set value");

    let out = NamedTempFile::new().expect("create out");
    editor
        .save_with_options(out.path().to_str().unwrap(), SaveOptions::incremental())
        .expect("save incremental");

    let reopened = PdfDocument::open(out.path().to_str().unwrap()).expect("reopen");
    let fields = FormExtractor::extract_fields(&reopened).expect("extract fields");
    let name_field = fields.iter().find(|f| f.full_name == "name");
    assert!(name_field.is_some(), "name field should exist after save");
    assert_eq!(
        name_field.unwrap().value,
        FieldValue::Text("Jane Doe".into()),
        "Saved text value should persist after incremental save"
    );
}

#[test]
fn test_save_incremental_persists_checkbox() {
    use xberg_native_pdf::editor::form_fields::FormFieldValue;
    use xberg_native_pdf::editor::{DocumentEditor, EditableDocument, SaveOptions};

    let bytes = create_form_pdf_bytes();
    let mut temp = NamedTempFile::new().expect("create temp");
    temp.write_all(&bytes).expect("write temp");

    let mut editor = DocumentEditor::open(temp.path().to_str().unwrap()).expect("open editor");
    editor
        .set_form_field_value("newsletter", FormFieldValue::Boolean(true))
        .expect("set checkbox");

    let out = NamedTempFile::new().expect("create out");
    editor
        .save_with_options(out.path().to_str().unwrap(), SaveOptions::incremental())
        .expect("save incremental");

    let reopened = PdfDocument::open(out.path().to_str().unwrap()).expect("reopen");
    let fields = FormExtractor::extract_fields(&reopened).expect("extract fields");
    let newsletter = fields.iter().find(|f| f.full_name == "newsletter");
    assert!(newsletter.is_some(), "newsletter field should exist");

    let val = &newsletter.unwrap().value;
    let is_checked = matches!(val, FieldValue::Boolean(true))
        || matches!(val, FieldValue::Name(n) if n == "Yes");
    assert!(is_checked, "Checkbox should be checked after save, got {:?}", val);

    let text = reopened.extract_text(0).expect("extract text");
    let checked_count = text.matches("[x]").count();
    assert!(
        checked_count >= 2,
        "Expected at least 2 [x] in text, got {}.\nText: {}",
        checked_count,
        text
    );
}

#[test]
fn test_save_incremental_text_value_inline() {
    use xberg_native_pdf::editor::form_fields::FormFieldValue;
    use xberg_native_pdf::editor::{DocumentEditor, EditableDocument, SaveOptions};

    let bytes = create_form_pdf_bytes();
    let mut temp = NamedTempFile::new().expect("create temp");
    temp.write_all(&bytes).expect("write temp");

    let mut editor = DocumentEditor::open(temp.path().to_str().unwrap()).expect("open editor");
    editor
        .set_form_field_value("name", FormFieldValue::Text("Alice Smith".into()))
        .expect("set value");

    let out = NamedTempFile::new().expect("create out");
    editor
        .save_with_options(out.path().to_str().unwrap(), SaveOptions::incremental())
        .expect("save incremental");

    let reopened = PdfDocument::open(out.path().to_str().unwrap()).expect("reopen");
    let text = reopened.extract_text(0).expect("extract text");
    assert!(
        text.contains("Alice Smith"),
        "Filled text value should appear inline in extract_text.\nGot: {}",
        text
    );
}
