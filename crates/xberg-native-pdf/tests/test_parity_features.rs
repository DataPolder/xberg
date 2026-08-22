use xberg_native_pdf::api::Pdf;

#[test]
fn test_word_extraction() {
    let mut pdf = Pdf::from_text("Hello World").unwrap();
    let words = pdf.extract_words(0).unwrap();

    println!(
        "Extracted words: {:?}",
        words.iter().map(|w| &w.text).collect::<Vec<_>>()
    );

    assert!(words.len() >= 2, "Expected at least 2 words, found {}", words.len());
    let texts: Vec<String> = words.iter().map(|w| w.text.trim().to_string()).collect();
    assert!(
        texts.iter().any(|t| t == "Hello"),
        "Could not find 'Hello' in {:?}",
        texts
    );
    assert!(
        texts.iter().any(|t| t == "World"),
        "Could not find 'World' in {:?}",
        texts
    );
}

#[test]
fn test_line_extraction() {
    let mut pdf = Pdf::from_text("Line One\n\nLine Two\n\nLine Three").unwrap();
    let lines = pdf.extract_text_lines(0).unwrap();

    println!(
        "Extracted lines: {:?}",
        lines.iter().map(|l| &l.text).collect::<Vec<_>>()
    );

    assert!(lines.len() >= 3, "Expected at least 3 lines, found {}", lines.len());
    let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
    assert!(texts.iter().any(|t| t.contains("Line One")));
    assert!(texts.iter().any(|t| t.contains("Line Two")));
}

#[test]
fn test_rect_and_line_extraction_empty() {
    let mut pdf = Pdf::from_text("Test").unwrap();
    let rects = pdf.extract_rects(0).unwrap();
    let lines = pdf.extract_lines(0).unwrap();

    assert!(rects.is_empty());
    assert!(lines.is_empty());
}

#[test]
fn test_table_extraction_basic() {
    let mut pdf = Pdf::from_markdown("| Col1 | Col2 |\n|---|---|\n| Val1 | Val2 |").unwrap();

    let spans = pdf.extract_spans(0).unwrap();
    println!("Spans found: {}", spans.len());
    for s in &spans {
        println!("  '{}' at {:?}", s.text, s.bbox);
    }

    let tables = pdf.extract_tables(0).unwrap();

    assert!(!tables.is_empty(), "No tables detected in markdown-generated PDF");
}

#[test]
fn test_area_filtered_extraction() {
    let mut pdf = Pdf::from_text("Top Text\n\n\n\n\nBottom Text").unwrap();

    let chars = pdf.extract_chars(0).unwrap();
    println!("Chars found: {}", chars.len());
    for c in &chars {
        println!("  '{}' at {:?}", c.char, c.bbox);
    }

    // Extract only from top region
    // Margin top is usually 72.0 (1 inch)
    // Page height is 792.0
    // start_y is 792 - 72 = 720.0 ~keep
    let top_rect = xberg_native_pdf::geometry::Rect::new(0.0, 700.0, 612.0, 92.0);
    let top_text = pdf
        .extract_text_in_rect(0, top_rect, xberg_native_pdf::layout::RectFilterMode::Intersects)
        .unwrap();
    println!("Top text: '{}'", top_text);

    assert!(top_text.contains("Top Text"));
    assert!(!top_text.contains("Bottom Text"));

    let bottom_rect = xberg_native_pdf::geometry::Rect::new(0.0, 0.0, 612.0, 650.0);
    let bottom_text = pdf
        .extract_text_in_rect(0, bottom_rect, xberg_native_pdf::layout::RectFilterMode::Intersects)
        .unwrap();
    println!("Bottom text: '{}'", bottom_text);

    assert!(!bottom_text.contains("Top Text"));
    assert!(bottom_text.contains("Bottom Text"));
}

#[test]
fn test_within_harmonized_api() {
    let mut pdf = Pdf::from_text("Scoped Content").unwrap();
    let rect = xberg_native_pdf::geometry::Rect::new(0.0, 0.0, 612.0, 792.0);

    let text = pdf.within(0, rect).extract_text().unwrap();
    assert!(text.contains("Scoped Content"));

    let words = pdf.within(0, rect).extract_words().unwrap();
    assert!(!words.is_empty());
}

#[test]
fn test_image_metadata_extraction() {
    let mut pdf = Pdf::from_text("No Images").unwrap();
    let images = pdf.extract_images(0).unwrap();
    assert!(images.is_empty());
}
