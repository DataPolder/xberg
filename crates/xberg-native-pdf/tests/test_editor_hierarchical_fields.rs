//! Integration tests for hierarchical form field creation in DocumentEditor.
//!
//! Tests the ability to create parent/child field structures in PDF documents.

use std::fs;
use tempfile::tempdir;
use xberg_native_pdf::editor::{DocumentEditor, EditableDocument, FormFieldType, ParentFieldConfig};
use xberg_native_pdf::geometry::Rect;
use xberg_native_pdf::writer::form_fields::TextFieldWidget;

/// Test creating parent and child fields manually using add_parent_field and add_child_field.
#[test]
fn test_create_parent_child_manual() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let parent_name = editor.add_parent_field(ParentFieldConfig::new("address")).unwrap();
    assert_eq!(parent_name, "address");

    let child1_name = editor
        .add_child_field(
            "address",
            0,
            TextFieldWidget::new("street", Rect::new(100.0, 700.0, 200.0, 20.0)),
        )
        .unwrap();
    assert_eq!(child1_name, "address.street");

    let child2_name = editor
        .add_child_field(
            "address",
            0,
            TextFieldWidget::new("city", Rect::new(100.0, 670.0, 150.0, 20.0)),
        )
        .unwrap();
    assert_eq!(child2_name, "address.city");

    assert!(editor.has_form_field("address").unwrap());
    assert!(editor.has_form_field("address.street").unwrap());
    assert!(editor.has_form_field("address.city").unwrap());

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("hierarchical_manual.pdf");
    editor.save(&output_path).unwrap();

    assert!(output_path.exists());
    let contents = fs::read(&output_path).unwrap();
    assert!(contents.len() > 100);
}

/// Test automatic hierarchical field creation using add_form_field_hierarchical.
#[test]
fn test_create_hierarchical_automatic() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let name1 = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("contact.email", Rect::new(100.0, 700.0, 200.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name1, "contact.email");

    let name2 = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("contact.phone", Rect::new(100.0, 670.0, 150.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name2, "contact.phone");

    assert!(editor.has_form_field("contact").unwrap());
    assert!(editor.has_form_field("contact.email").unwrap());
    assert!(editor.has_form_field("contact.phone").unwrap());

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("hierarchical_auto.pdf");
    editor.save(&output_path).unwrap();

    assert!(output_path.exists());
}

/// Test three-level hierarchy: parent.child.grandchild.
#[test]
fn test_three_level_hierarchy() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let name1 = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("person.address.street", Rect::new(100.0, 700.0, 200.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name1, "person.address.street");

    let name2 = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("person.address.city", Rect::new(100.0, 670.0, 150.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name2, "person.address.city");

    let name3 = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("person.name", Rect::new(100.0, 640.0, 200.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name3, "person.name");

    assert!(editor.has_form_field("person").unwrap());
    assert!(editor.has_form_field("person.address").unwrap());
    assert!(editor.has_form_field("person.address.street").unwrap());
    assert!(editor.has_form_field("person.address.city").unwrap());
    assert!(editor.has_form_field("person.name").unwrap());

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("three_level.pdf");
    editor.save(&output_path).unwrap();

    assert!(output_path.exists());
}

/// Test property inheritance from parent to child fields.
#[test]
fn test_property_inheritance() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let parent_name = editor
        .add_parent_field(ParentFieldConfig::new("form_data").with_field_type(FormFieldType::Text))
        .unwrap();
    assert_eq!(parent_name, "form_data");

    let child_name = editor
        .add_child_field(
            "form_data",
            0,
            TextFieldWidget::new("username", Rect::new(100.0, 700.0, 200.0, 20.0)),
        )
        .unwrap();
    assert_eq!(child_name, "form_data.username");

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("inheritance.pdf");
    editor.save(&output_path).unwrap();

    assert!(output_path.exists());
}

/// Test saving and reloading hierarchical fields to verify persistence.
#[test]
fn test_save_and_reload_hierarchy() {
    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("reload_test.pdf");

    {
        let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

        editor
            .add_form_field_hierarchical(
                0,
                TextFieldWidget::new("user.first_name", Rect::new(100.0, 700.0, 150.0, 20.0)).with_value("John"),
            )
            .unwrap();

        editor
            .add_form_field_hierarchical(
                0,
                TextFieldWidget::new("user.last_name", Rect::new(100.0, 670.0, 150.0, 20.0)).with_value("Doe"),
            )
            .unwrap();

        editor.save(&output_path).unwrap();
    }

    {
        let mut editor = DocumentEditor::open(&output_path).unwrap();

        let fields = editor.get_form_fields().unwrap();
        let field_names: Vec<_> = fields.iter().map(|f| f.name()).collect();

        assert!(
            field_names.contains(&"user.first_name"),
            "Expected user.first_name, got: {:?}",
            field_names
        );
        assert!(
            field_names.contains(&"user.last_name"),
            "Expected user.last_name, got: {:?}",
            field_names
        );
    }
}

/// Test that flat fields (no hierarchy) still work correctly.
#[test]
fn test_flat_field_via_hierarchical_api() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let name = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("simple_field", Rect::new(100.0, 700.0, 200.0, 20.0)),
        )
        .unwrap();

    assert_eq!(name, "simple_field");
    assert!(editor.has_form_field("simple_field").unwrap());

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("flat_via_hier.pdf");
    editor.save(&output_path).unwrap();

    assert!(output_path.exists());
}

/// Test error when adding child to non-existent parent.
#[test]
fn test_add_child_nonexistent_parent() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let result = editor.add_child_field(
        "nonexistent_parent",
        0,
        TextFieldWidget::new("child", Rect::new(100.0, 700.0, 200.0, 20.0)),
    );

    assert!(result.is_err());
}

/// Test duplicate parent name returns error.
#[test]
fn test_duplicate_parent_name_error() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    let name1 = editor.add_parent_field(ParentFieldConfig::new("parent")).unwrap();
    assert_eq!(name1, "parent");

    let result = editor.add_parent_field(ParentFieldConfig::new("parent"));
    assert!(result.is_err());

    assert!(editor.has_form_field("parent").unwrap());
}

/// Test mixed manual and automatic hierarchy creation.
#[test]
fn test_mixed_hierarchy_creation() {
    let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").unwrap();

    editor.add_parent_field(ParentFieldConfig::new("billing")).unwrap();

    let name1 = editor
        .add_form_field_hierarchical(
            0,
            TextFieldWidget::new("billing.address", Rect::new(100.0, 700.0, 200.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name1, "billing.address");

    let name2 = editor
        .add_child_field(
            "billing",
            0,
            TextFieldWidget::new("amount", Rect::new(100.0, 670.0, 100.0, 20.0)),
        )
        .unwrap();
    assert_eq!(name2, "billing.amount");

    assert!(editor.has_form_field("billing").unwrap());
    assert!(editor.has_form_field("billing.address").unwrap());
    assert!(editor.has_form_field("billing.amount").unwrap());

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("mixed.pdf");
    editor.save(&output_path).unwrap();

    assert!(output_path.exists());
}
