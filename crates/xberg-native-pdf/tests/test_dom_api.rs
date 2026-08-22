//! Integration tests for the DOM-like PDF editing API.

#[cfg(test)]
mod dom_api_tests {
    use xberg_native_pdf::editor::{ElementId, PdfPage};
    use xberg_native_pdf::elements::{ContentElement, FontSpec, StructureElement, TextContent, TextStyle};
    use xberg_native_pdf::geometry::Rect;

    /// Create a test page with some sample content.
    fn create_test_page() -> PdfPage {
        let text1 = TextContent {
            text: "Hello World".to_string(),
            bbox: Rect::new(72.0, 720.0, 100.0, 12.0),
            font: FontSpec::new("Helvetica", 12.0),
            style: TextStyle::new(),
            ..Default::default()
        };

        let text2 = TextContent {
            text: "This is a test".to_string(),
            bbox: Rect::new(72.0, 700.0, 100.0, 12.0),
            font: FontSpec::new("Helvetica", 12.0),
            style: TextStyle::new(),
            ..Default::default()
        };

        let children = vec![ContentElement::Text(text1), ContentElement::Text(text2)];

        let root = StructureElement {
            structure_type: "Document".to_string(),
            bbox: Rect::new(0.0, 0.0, 612.0, 792.0),
            children,
            reading_order: Some(0),
            alt_text: None,
            language: None,
        };

        PdfPage::from_structure(0, root, 612.0, 792.0)
    }

    #[test]
    fn test_find_text_containing() {
        let page = create_test_page();

        let results = page.find_text_containing("Hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text(), "Hello World");
    }

    #[test]
    fn test_find_text_with_predicate() {
        let page = create_test_page();

        let results = page.find_text(|t| t.text().starts_with("This"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text(), "This is a test");
    }

    #[test]
    fn test_find_text_no_match() {
        let page = create_test_page();

        let results = page.find_text_containing("Nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_pdf_text_properties() {
        let page = create_test_page();

        let results = page.find_text_containing("Hello");
        assert_eq!(results.len(), 1);

        let text = &results[0];
        assert_eq!(text.text(), "Hello World");
        assert_eq!(text.bbox().x, 72.0);
        assert_eq!(text.bbox().y, 720.0);
        assert_eq!(text.font_name(), "Helvetica");
        assert_eq!(text.font_size(), 12.0);
        assert!(!text.is_bold());
        assert!(!text.is_italic());
    }

    #[test]
    fn test_element_id_uniqueness() {
        let id1 = ElementId::new();
        let id2 = ElementId::new();

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_children_access() {
        let page = create_test_page();

        let children = page.children();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_match_pdf_element() {
        let page = create_test_page();
        let children = page.children();

        for element in children {
            match element {
                xberg_native_pdf::editor::PdfElement::Text(text) => {
                    assert!(!text.text().is_empty());
                }
                _ => panic!("Expected Text element"),
            }
        }
    }

    #[test]
    fn test_page_dimensions() {
        let page = create_test_page();

        assert_eq!(page.width, 612.0);
        assert_eq!(page.height, 792.0);
    }

    #[test]
    fn test_element_bbox() {
        let page = create_test_page();

        let results = page.find_text_containing("Hello");
        assert!(!results.is_empty());

        let bbox = results[0].bbox();
        assert_eq!(bbox.x, 72.0);
        assert_eq!(bbox.y, 720.0);
        assert_eq!(bbox.width, 100.0);
        assert_eq!(bbox.height, 12.0);
    }

    #[test]
    fn test_find_in_region() {
        let page = create_test_page();

        let region = Rect::new(0.0, 710.0, 200.0, 30.0);
        let elements = page.find_in_region(region);

        assert!(!elements.is_empty());
        if let Some(text) = elements[0].as_text() {
            assert_eq!(text.text(), "Hello World");
        }
    }

    #[test]
    fn test_find_in_region_no_match() {
        let page = create_test_page();

        let region = Rect::new(0.0, 0.0, 50.0, 50.0);
        let elements = page.find_in_region(region);

        assert!(elements.is_empty());
    }

    #[test]
    fn test_pdf_page_creation() {
        let page = create_test_page();

        assert_eq!(page.page_index, 0);
        assert_eq!(page.width, 612.0);
        assert_eq!(page.height, 792.0);
    }

    #[test]
    fn test_all_find_methods() {
        let page = create_test_page();

        let by_contains = page.find_text_containing("Hello");
        assert!(!by_contains.is_empty());

        let by_predicate = page.find_text(|t| t.text().contains("Hello"));
        assert!(!by_predicate.is_empty());

        assert_eq!(by_contains.len(), by_predicate.len());
    }

    #[test]
    fn test_empty_page() {
        let root = StructureElement {
            structure_type: "Document".to_string(),
            bbox: Rect::new(0.0, 0.0, 612.0, 792.0),
            children: Vec::new(),
            reading_order: Some(0),
            alt_text: None,
            language: None,
        };

        let page = PdfPage::from_structure(0, root, 612.0, 792.0);

        let results = page.find_text_containing("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_fluent_api_find_and_modify() {
        let page = create_test_page();

        let editor = xberg_native_pdf::editor::PageEditor { page };
        let result = editor
            .find_text_containing("Hello")
            .expect("find_text_containing failed");

        let modified = result
            .for_each(|text| {
                text.set_text("Hi");
                Ok(())
            })
            .expect("for_each failed")
            .done()
            .expect("done failed");

        let modified_text = modified.find_text_containing("Hi");
        assert_eq!(modified_text.len(), 1);
        assert_eq!(modified_text[0].text(), "Hi");
    }

    #[test]
    fn test_fluent_api_chaining() {
        let page = create_test_page();

        let result = xberg_native_pdf::editor::PageEditor { page }
            .find_text_containing("Hello")
            .expect("find_text_containing failed")
            .for_each(|text| {
                text.set_text("Modified");
                Ok(())
            })
            .expect("for_each failed")
            .done();

        assert!(result.is_ok());
        let modified_page = result.unwrap();
        let modified_text = modified_page.find_text_containing("Modified");
        assert_eq!(modified_text.len(), 1);
    }

    #[test]
    fn test_fluent_api_with_predicate() {
        let page = create_test_page();

        let result = xberg_native_pdf::editor::PageEditor { page }
            .find_text(|t| t.text().starts_with("Hello"))
            .expect("find_text failed")
            .for_each(|text| {
                text.set_text("Updated");
                Ok(())
            })
            .expect("for_each failed")
            .done();

        assert!(result.is_ok());
        let modified_page = result.unwrap();
        let updated_text = modified_page.find_text_containing("Updated");
        assert_eq!(updated_text.len(), 1);
        assert_eq!(updated_text[0].text(), "Updated");
    }

    #[test]
    fn test_dom_with_real_pdf() {
        use xberg_native_pdf::editor::DocumentEditor;

        let mut editor = DocumentEditor::open("tests/fixtures/simple.pdf").expect("Failed to open test PDF");

        let page = editor.get_page(0).expect("Failed to get page 0");

        let all_text = page.find_text(|_| true);
        println!("Found {} text elements on simple.pdf", all_text.len());

        // Note: simple.pdf is a minimal test fixture that may have no text content
        // This test verifies the DOM API works, not that there's text in this file
        // Print first few text elements for debugging ~keep
        for (i, t) in all_text.iter().take(3).enumerate() {
            println!("  [{}] '{}' at ({:.1}, {:.1})", i, t.text(), t.bbox().x, t.bbox().y);
        }
    }

    #[test]
    fn test_dom_text_modification_in_memory() {
        let page = create_test_page();

        let texts = page.find_text(|_| true);
        assert!(!texts.is_empty(), "Test page should have text elements");

        let first_id = texts[0].id();
        let original_text = texts[0].text().to_string();

        let mut page = page;

        page.set_text(first_id, "MODIFIED").expect("Failed to set text");

        if let Some(element) = page.get_element(first_id)
            && let Some(text) = element.as_text()
        {
            assert_eq!(text.text(), "MODIFIED");
            println!("Successfully modified text from '{}' to 'MODIFIED'", original_text);
        }
    }

    #[test]
    fn test_dom_round_trip_create_and_read() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use xberg_native_pdf::editor::DocumentEditor;
        use xberg_native_pdf::writer::{DocumentBuilder, PageSize};

        let mut builder = DocumentBuilder::new();
        builder
            .page(PageSize::Letter)
            .at(72.0, 720.0)
            .font("Helvetica", 24.0)
            .text("Hello PDF World")
            .at(72.0, 680.0)
            .font("Helvetica", 12.0)
            .text("This is a test document")
            .done();
        builder
            .page(PageSize::Letter)
            .at(72.0, 700.0)
            .text("Page 2 content")
            .done();
        let pdf_bytes = builder.build().expect("Failed to build PDF");

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(&pdf_bytes).expect("Failed to write PDF");
        let path = temp_file.path().to_path_buf();

        let mut editor = DocumentEditor::open(&path).expect("Failed to open generated PDF");

        let page0 = editor.get_page(0).expect("Failed to get page 0");
        let texts = page0.find_text(|_| true);

        println!("Found {} text elements on page 0", texts.len());
        for (i, t) in texts.iter().enumerate() {
            println!("  [{}] '{}' at ({:.1}, {:.1})", i, t.text(), t.bbox().x, t.bbox().y);
        }

        assert!(!texts.is_empty(), "Should find text on the generated PDF page");

        let hello_texts = page0.find_text_containing("Hello");
        assert!(!hello_texts.is_empty(), "Should find 'Hello' on the page");
        println!("Found 'Hello' in: '{}'", hello_texts[0].text());

        let page1 = editor.get_page(1).expect("Failed to get page 1");
        let page1_texts = page1.find_text(|_| true);
        assert!(!page1_texts.is_empty(), "Should find text on page 1");
        println!("Page 1 has {} text elements", page1_texts.len());
    }

    #[test]
    fn test_unified_pdf_api_metadata() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use xberg_native_pdf::api::Pdf;
        use xberg_native_pdf::writer::{DocumentBuilder, PageSize};

        let mut builder = DocumentBuilder::new();
        builder
            .page(PageSize::Letter)
            .at(72.0, 720.0)
            .font("Helvetica", 12.0)
            .text("Test document")
            .done();
        let pdf_bytes = builder.build().expect("Failed to build PDF");

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(&pdf_bytes).expect("Failed to write PDF");
        let path = temp_file.path();

        let mut pdf = Pdf::open(path).expect("Failed to open PDF");

        pdf.set_title("Test Title").expect("Failed to set title");
        pdf.set_author("Test Author").expect("Failed to set author");
        pdf.set_subject("Test Subject").expect("Failed to set subject");
        pdf.set_keywords("test, pdf, metadata").expect("Failed to set keywords");

        assert!(pdf.is_modified(), "Document should be modified after setting metadata");
    }

    #[test]
    fn test_unified_pdf_api_page_access() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use xberg_native_pdf::api::Pdf;
        use xberg_native_pdf::writer::{DocumentBuilder, PageSize};

        let mut builder = DocumentBuilder::new();
        builder
            .page(PageSize::Letter)
            .at(72.0, 720.0)
            .font("Helvetica", 18.0)
            .text("Hello World")
            .done();
        let pdf_bytes = builder.build().expect("Failed to build PDF");

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(&pdf_bytes).expect("Failed to write PDF");
        let path = temp_file.path();

        let mut pdf = Pdf::open(path).expect("Failed to open PDF");

        let count = pdf.page_count().expect("Failed to get page count");
        assert_eq!(count, 1, "Should have 1 page");

        let page = pdf.page(0).expect("Failed to get page 0");

        let texts = page.find_text_containing("Hello");
        assert!(!texts.is_empty(), "Should find 'Hello' on the page");
    }

    #[test]
    fn test_page_add_and_remove_text() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use xberg_native_pdf::api::Pdf;
        use xberg_native_pdf::api::TextContent;
        use xberg_native_pdf::writer::{DocumentBuilder, PageSize};

        let mut builder = DocumentBuilder::new();
        builder
            .page(PageSize::Letter)
            .at(72.0, 720.0)
            .font("Helvetica", 12.0)
            .text("Original")
            .done();
        let pdf_bytes = builder.build().expect("Failed to build PDF");

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(&pdf_bytes).expect("Failed to write PDF");
        let path = temp_file.path();

        let mut pdf = Pdf::open(path).expect("Failed to open PDF");

        {
            let mut page = pdf.page(0).expect("Failed to get page");

            let text_content = TextContent {
                text: "New Text".to_string(),
                bbox: Rect::new(100.0, 650.0, 80.0, 14.0),
                ..Default::default()
            };
            let text_id = page.add_text(text_content);

            let new_texts = page.find_text_containing("New Text");
            assert!(!new_texts.is_empty(), "Should find newly added text");

            let removed = page.remove_element(text_id);
            assert!(removed, "Should successfully remove the element");

            let after_remove = page.find_text_containing("New Text");
            assert!(after_remove.is_empty(), "Text should be removed");
        }
    }

    #[test]
    fn test_page_annotations() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        use xberg_native_pdf::api::{LinkAnnotation, Pdf, Rect as ApiRect, TextAnnotation};
        use xberg_native_pdf::writer::{DocumentBuilder, PageSize};

        let mut builder = DocumentBuilder::new();
        builder
            .page(PageSize::Letter)
            .at(72.0, 720.0)
            .font("Helvetica", 12.0)
            .text("Document with annotations")
            .done();
        let pdf_bytes = builder.build().expect("Failed to build PDF");

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(&pdf_bytes).expect("Failed to write PDF");
        let path = temp_file.path();

        let mut pdf = Pdf::open(path).expect("Failed to open PDF");

        {
            let mut page = pdf.page(0).expect("Failed to get page");

            let link = LinkAnnotation::uri(ApiRect::new(72.0, 700.0, 100.0, 20.0), "https://example.com");
            let _link_id = page.add_annotation(link);

            let note = TextAnnotation::new(ApiRect::new(200.0, 700.0, 20.0, 20.0), "This is a comment");
            let _note_id = page.add_annotation(note);

            let annotations = page.annotations();
            assert!(annotations.len() >= 2, "Should have at least 2 annotations");

            let first = &annotations[0];
            let rect = first.rect();
            assert!(rect.x >= 0.0 && rect.width > 0.0, "Rect should have valid dimensions");
        }
    }
}
