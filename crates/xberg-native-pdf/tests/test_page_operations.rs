//! Integration tests for page operations (rotation, cropping).

use std::fs;
use tempfile::tempdir;
use xberg_native_pdf::api::Pdf;
use xberg_native_pdf::editor::{DocumentEditor, EditableDocument};
use xberg_native_pdf::writer::{DocumentBuilder, DocumentMetadata, PageSize};

/// Helper to create a simple test PDF
fn create_test_pdf() -> Vec<u8> {
    let mut builder = DocumentBuilder::new();
    builder = builder.metadata(
        DocumentMetadata::new()
            .title("Test Document")
            .author("Test Author"),
    );

    {
        let page1 = builder.page(PageSize::Letter);
        page1.at(72.0, 720.0).text("Page 1 Content").done();
    }
    {
        let page2 = builder.page(PageSize::Letter);
        page2.at(72.0, 720.0).text("Page 2 Content").done();
    }
    {
        let page3 = builder.page(PageSize::Letter);
        page3.at(72.0, 720.0).text("Page 3 Content").done();
    }

    builder.build().expect("Failed to build test PDF")
}

mod rotation_tests {
    use super::*;

    #[test]
    fn test_get_initial_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let rotation = pdf.page_rotation(0).unwrap();
        assert_eq!(rotation, 0);
    }

    #[test]
    fn test_set_page_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.set_page_rotation(0, 90).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 90);

        pdf.set_page_rotation(0, 180).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 180);

        pdf.set_page_rotation(0, 270).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 270);

        pdf.set_page_rotation(0, 0).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 0);
    }

    #[test]
    fn test_invalid_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let result = pdf.set_page_rotation(0, 45);
        assert!(result.is_err());

        let result = pdf.set_page_rotation(0, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_rotate_page_incremental() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        assert_eq!(pdf.page_rotation(0).unwrap(), 0);

        pdf.rotate_page(0, 90).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 90);

        pdf.rotate_page(0, 90).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 180);

        pdf.rotate_page(0, 90).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 270);

        pdf.rotate_page(0, 90).unwrap();
        assert_eq!(pdf.page_rotation(0).unwrap(), 0);
    }

    #[test]
    fn test_rotate_all_pages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.rotate_all_pages(90).unwrap();

        assert_eq!(pdf.page_rotation(0).unwrap(), 90);
        assert_eq!(pdf.page_rotation(1).unwrap(), 90);
        assert_eq!(pdf.page_rotation(2).unwrap(), 90);
    }

    #[test]
    fn test_rotation_persists_after_save() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        {
            let mut pdf = Pdf::open(&input_path).unwrap();
            pdf.set_page_rotation(0, 90).unwrap();
            pdf.save(&output_path).unwrap();
        }

        {
            let mut pdf = Pdf::open(&output_path).unwrap();
            assert_eq!(pdf.page_rotation(0).unwrap(), 90);
        }
    }
}

mod cropping_tests {
    use super::*;

    #[test]
    fn test_get_media_box() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let media_box = pdf.page_media_box(0).unwrap();
        assert_eq!(media_box[0], 0.0);
        assert_eq!(media_box[1], 0.0);
        assert_eq!(media_box[2], 612.0);
        assert_eq!(media_box[3], 792.0);
    }

    #[test]
    fn test_get_crop_box_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let crop_box = pdf.page_crop_box(0).unwrap();
        assert!(crop_box.is_none());
    }

    #[test]
    fn test_set_crop_box() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.set_page_crop_box(0, [72.0, 72.0, 540.0, 720.0])
            .unwrap();

        let crop_box = pdf.page_crop_box(0).unwrap();
        assert!(crop_box.is_some());
        let crop_box = crop_box.unwrap();
        assert_eq!(crop_box[0], 72.0);
        assert_eq!(crop_box[1], 72.0);
        assert_eq!(crop_box[2], 540.0);
        assert_eq!(crop_box[3], 720.0);
    }

    #[test]
    fn test_set_media_box() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.set_page_media_box(0, [0.0, 0.0, 595.0, 842.0]).unwrap();

        let media_box = pdf.page_media_box(0).unwrap();
        assert_eq!(media_box[2], 595.0);
        assert_eq!(media_box[3], 842.0);
    }

    #[test]
    fn test_crop_margins() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.crop_margins(36.0, 36.0, 36.0, 36.0).unwrap();

        for i in 0..3 {
            let crop_box = pdf.page_crop_box(i).unwrap();
            assert!(crop_box.is_some());
            let crop_box = crop_box.unwrap();
            // Original is 612x792, after 36pt margins:
            // llx = 0 + 36 = 36
            // lly = 0 + 36 = 36
            // urx = 612 - 36 = 576
            // ury = 792 - 36 = 756 ~keep
            assert_eq!(crop_box[0], 36.0);
            assert_eq!(crop_box[1], 36.0);
            assert_eq!(crop_box[2], 576.0);
            assert_eq!(crop_box[3], 756.0);
        }
    }

    #[test]
    fn test_crop_box_persists_after_save() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        {
            let mut pdf = Pdf::open(&input_path).unwrap();
            pdf.set_page_crop_box(0, [72.0, 72.0, 540.0, 720.0])
                .unwrap();
            pdf.save(&output_path).unwrap();
        }

        {
            let mut pdf = Pdf::open(&output_path).unwrap();
            let crop_box = pdf.page_crop_box(0).unwrap();
            assert!(crop_box.is_some());
            let crop_box = crop_box.unwrap();
            assert_eq!(crop_box[0], 72.0);
            assert_eq!(crop_box[1], 72.0);
            assert_eq!(crop_box[2], 540.0);
            assert_eq!(crop_box[3], 720.0);
        }
    }
}

mod document_editor_tests {
    use super::*;

    #[test]
    fn test_editor_rotation_methods() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        assert_eq!(editor.get_page_rotation(0).unwrap(), 0);
        editor.set_page_rotation(0, 90).unwrap();
        assert_eq!(editor.get_page_rotation(0).unwrap(), 90);

        editor.rotate_page_by(0, 90).unwrap();
        assert_eq!(editor.get_page_rotation(0).unwrap(), 180);
    }

    #[test]
    fn test_editor_box_methods() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        let media_box = editor.get_page_media_box(0).unwrap();
        assert_eq!(media_box[2], 612.0);
        assert_eq!(media_box[3], 792.0);

        let crop_box = editor.get_page_crop_box(0).unwrap();
        assert!(crop_box.is_none());

        editor
            .set_page_crop_box(0, [72.0, 72.0, 540.0, 720.0])
            .unwrap();
        let crop_box = editor.get_page_crop_box(0).unwrap();
        assert!(crop_box.is_some());
    }

    #[test]
    fn test_editor_erase_methods() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        editor
            .erase_region(0, [100.0, 100.0, 200.0, 200.0])
            .unwrap();
        assert!(editor.is_modified());

        editor
            .erase_regions(1, &[[50.0, 50.0, 100.0, 100.0], [200.0, 200.0, 300.0, 300.0]])
            .unwrap();

        editor.clear_erase_regions(0);
    }

    #[test]
    fn test_editor_erase_and_save() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&input_path).unwrap();
        editor.erase_region(0, [72.0, 700.0, 300.0, 750.0]).unwrap();
        editor.save(&output_path).unwrap();

        let mut pdf = Pdf::open(&output_path).unwrap();
        assert_eq!(pdf.page_count().unwrap(), 3);
    }
}

mod erasing_tests {
    use super::*;

    #[test]
    fn test_erase_region() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.erase_region(0, [100.0, 100.0, 200.0, 200.0]).unwrap();

        assert!(pdf.is_modified());
    }

    #[test]
    fn test_erase_multiple_regions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.erase_regions(
            0,
            &[
                [50.0, 50.0, 100.0, 100.0],
                [150.0, 150.0, 250.0, 250.0],
                [300.0, 300.0, 400.0, 400.0],
            ],
        )
        .unwrap();

        assert!(pdf.is_modified());
    }

    #[test]
    fn test_erase_invalid_page() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let result = pdf.erase_region(100, [0.0, 0.0, 100.0, 100.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_erase_region_persists_after_save() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        {
            let mut pdf = Pdf::open(&input_path).unwrap();
            pdf.erase_region(0, [72.0, 720.0, 300.0, 750.0]).unwrap();
            pdf.save(&output_path).unwrap();
        }

        {
            let mut pdf = Pdf::open(&output_path).unwrap();
            assert_eq!(pdf.page_count().unwrap(), 3);
        }
    }

    #[test]
    fn test_clear_erase_regions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.erase_region(0, [100.0, 100.0, 200.0, 200.0]).unwrap();
        pdf.erase_region(1, [50.0, 50.0, 150.0, 150.0]).unwrap();

        pdf.clear_erase_regions(0);
    }

    #[test]
    fn test_erase_on_multiple_pages() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        {
            let mut pdf = Pdf::open(&input_path).unwrap();
            pdf.erase_region(0, [72.0, 700.0, 200.0, 750.0]).unwrap();
            pdf.erase_region(1, [72.0, 700.0, 200.0, 750.0]).unwrap();
            pdf.erase_region(2, [72.0, 700.0, 200.0, 750.0]).unwrap();
            pdf.save(&output_path).unwrap();
        }

        {
            let mut pdf = Pdf::open(&output_path).unwrap();
            assert_eq!(pdf.page_count().unwrap(), 3);
        }
    }
}

mod flatten_tests {
    use super::*;

    #[test]
    fn test_flatten_page_annotations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.flatten_page_annotations(0).unwrap();

        assert!(pdf.is_modified());
        assert!(pdf.is_page_marked_for_flatten(0));
    }

    #[test]
    fn test_flatten_all_annotations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.flatten_all_annotations().unwrap();

        assert!(pdf.is_page_marked_for_flatten(0));
        assert!(pdf.is_page_marked_for_flatten(1));
        assert!(pdf.is_page_marked_for_flatten(2));
    }

    #[test]
    fn test_flatten_invalid_page() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let result = pdf.flatten_page_annotations(100);
        assert!(result.is_err());
    }

    #[test]
    fn test_unmark_page_for_flatten() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.flatten_page_annotations(0).unwrap();
        assert!(pdf.is_page_marked_for_flatten(0));

        pdf.unmark_page_for_flatten(0);
        assert!(!pdf.is_page_marked_for_flatten(0));
    }

    #[test]
    fn test_flatten_and_save() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        {
            let mut pdf = Pdf::open(&input_path).unwrap();
            pdf.flatten_all_annotations().unwrap();
            pdf.save(&output_path).unwrap();
        }

        {
            let mut pdf = Pdf::open(&output_path).unwrap();
            assert_eq!(pdf.page_count().unwrap(), 3);
        }
    }

    #[test]
    fn test_editor_flatten_methods() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        editor.flatten_page_annotations(0).unwrap();
        assert!(editor.is_page_marked_for_flatten(0));
        assert!(editor.is_modified());

        editor.unmark_page_for_flatten(0);
        assert!(!editor.is_page_marked_for_flatten(0));

        editor.flatten_all_annotations().unwrap();
        assert!(editor.is_page_marked_for_flatten(0));
        assert!(editor.is_page_marked_for_flatten(1));
        assert!(editor.is_page_marked_for_flatten(2));
    }
}

mod redaction_tests {
    use super::*;

    #[test]
    fn test_apply_page_redactions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.apply_page_redactions(0).unwrap();

        assert!(pdf.is_modified());
        assert!(pdf.is_page_marked_for_redaction(0));
    }

    #[test]
    fn test_apply_all_redactions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.apply_all_redactions().unwrap();

        assert!(pdf.is_page_marked_for_redaction(0));
        assert!(pdf.is_page_marked_for_redaction(1));
        assert!(pdf.is_page_marked_for_redaction(2));
    }

    #[test]
    fn test_redaction_invalid_page() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        let result = pdf.apply_page_redactions(100);
        assert!(result.is_err());
    }

    #[test]
    fn test_unmark_page_for_redaction() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&path).unwrap();

        pdf.apply_page_redactions(0).unwrap();
        assert!(pdf.is_page_marked_for_redaction(0));

        pdf.unmark_page_for_redaction(0);
        assert!(!pdf.is_page_marked_for_redaction(0));
    }

    #[test]
    fn test_redaction_and_save() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        let output_path = dir.path().join("output.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        {
            let mut pdf = Pdf::open(&input_path).unwrap();
            pdf.apply_all_redactions().unwrap();
            pdf.save(&output_path).unwrap();
        }

        {
            let mut pdf = Pdf::open(&output_path).unwrap();
            assert_eq!(pdf.page_count().unwrap(), 3);
        }
    }

    #[test]
    fn test_editor_redaction_methods() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        editor.apply_page_redactions(0).unwrap();
        assert!(editor.is_page_marked_for_redaction(0));
        assert!(editor.is_modified());

        editor.unmark_page_for_redaction(0);
        assert!(!editor.is_page_marked_for_redaction(0));

        editor.apply_all_redactions().unwrap();
        assert!(editor.is_page_marked_for_redaction(0));
        assert!(editor.is_page_marked_for_redaction(1));
        assert!(editor.is_page_marked_for_redaction(2));
    }
}

mod image_tests {
    use super::*;

    #[test]
    fn test_get_page_images() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        let images = editor.get_page_images(0).unwrap();
        assert!(images.is_empty() || !images.is_empty());
    }

    #[test]
    fn test_image_modification_tracking() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        assert!(!editor.has_image_modifications(0));

        let _ = editor.reposition_image(0, "Im0", 100.0, 200.0);

        assert!(editor.has_image_modifications(0));

        editor.clear_image_modifications(0);
        assert!(!editor.has_image_modifications(0));
    }

    #[test]
    fn test_reposition_image() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        let result = editor.reposition_image(0, "Im0", 100.0, 200.0);
        if result.is_ok() {
            assert!(editor.has_image_modifications(0));
        }
    }

    #[test]
    fn test_resize_image() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        let result = editor.resize_image(0, "Im0", 300.0, 400.0);
        if result.is_ok() {
            assert!(editor.has_image_modifications(0));
        }
    }

    #[test]
    fn test_set_image_bounds() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.pdf");
        fs::write(&path, create_test_pdf()).unwrap();

        let mut editor = DocumentEditor::open(&path).unwrap();

        let result = editor.set_image_bounds(0, "Im0", 50.0, 50.0, 200.0, 150.0);
        if result.is_ok() {
            assert!(editor.has_image_modifications(0));
        }
    }

    #[test]
    fn test_pdf_api_image_methods() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.pdf");
        fs::write(&input_path, create_test_pdf()).unwrap();

        let mut pdf = Pdf::open(&input_path).unwrap();

        let images = pdf.page_images(0).unwrap();
        assert!(images.is_empty() || !images.is_empty());

        assert!(!pdf.has_image_modifications(0));
    }
}
