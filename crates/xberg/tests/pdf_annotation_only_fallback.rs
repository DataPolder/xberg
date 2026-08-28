#![cfg(feature = "pdf")]

use xberg::core::config::{ExtractInput, OcrConfig, PdfConfig};
use xberg::{ExtractionConfig, ResultFormat, extract};

const EXPECTED_TEXT: &str = "VISIBLE ANNOTATION TEXT";
const SECOND_PAGE_TEXT: &str = "PAGE TWO ANNOTATION";
const BODY_TEXT: &str = "BODY TEXT";
const PDF_MIME: &str = "application/pdf";
const FALLBACK_WARNING_SOURCE: &str = "pdf_annotations";
const FALLBACK_WARNING_MESSAGE: &str =
    "native PDF page text was empty; recovered 1 text-bearing annotation(s) as document content";
const INVISIBLE_ANNOTATION_FLAG: u32 = 1;
const HIDDEN_ANNOTATION_FLAG: u32 = 2;
const NO_VIEW_ANNOTATION_FLAG: u32 = 32;

struct AnnotationOptions<'a> {
    subtype: &'a str,
    flags: u32,
    opacity: Option<f64>,
    rect: [f64; 4],
    hidden_optional_content: bool,
}

impl Default for AnnotationOptions<'_> {
    fn default() -> Self {
        Self {
            subtype: "FreeText",
            flags: 0,
            opacity: None,
            rect: [100.0, 100.0, 320.0, 140.0],
            hidden_optional_content: false,
        }
    }
}

fn rotated_pdf(body_text: Option<&str>, annotation: AnnotationOptions<'_>) -> Vec<u8> {
    let appearance = format!("BT\n/Helv 14 Tf\n10 20 Td\n({EXPECTED_TEXT}) Tj\nET\n");
    let page_content = body_text
        .map(|text| format!("BT\n/Helv 14 Tf\n72 700 Td\n({text}) Tj\nET\n"))
        .unwrap_or_default();
    let opacity = annotation
        .opacity
        .map(|value| format!(" /CA {value}"))
        .unwrap_or_default();
    let optional_content = if annotation.hidden_optional_content {
        " /OC 8 0 R"
    } else {
        ""
    };
    let catalog_optional_content = if annotation.hidden_optional_content {
        " /OCProperties << /OCGs [8 0 R] /D << /OFF [8 0 R] >> >>"
    } else {
        ""
    };
    let [x0, y0, x1, y1] = annotation.rect;
    let objects = [
        format!("<< /Type /Catalog /Pages 2 0 R{catalog_optional_content} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate 90 \
         /Resources << /Font << /Helv 7 0 R >> >> /Contents 4 0 R /Annots [5 0 R] >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{page_content}endstream", page_content.len()),
        format!(
            "<< /Type /Annot /Subtype /{} /Rect [{x0} {y0} {x1} {y1}] \
             /Contents ({EXPECTED_TEXT}) /DA (0 0 0 rg /Helv 14 Tf) /Rotate 90 \
             /F {}{opacity}{optional_content} /AP << /N 6 0 R >> >>",
            annotation.subtype, annotation.flags
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 220 40] \
             /Resources << /Font << /Helv 7 0 R >> >> /Length {} >>\nstream\n{appearance}endstream",
            appearance.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
         /Encoding << /Type /Encoding /Differences \
         [32 /space 65 /A 66 /B 68 /D /E 73 /I 76 /L 78 /N /O /P 82 /R \
          84 /T 86 /V 88 /X /Y] >> >>"
            .to_string(),
        "<< /Type /OCG /Name (Hidden annotation layer) >>".to_string(),
    ];

    assemble_pdf(&objects)
}

fn two_page_pdf_with_second_page_annotation(rect: [f64; 4]) -> Vec<u8> {
    let appearance = format!("BT\n/Helv 14 Tf\n10 20 Td\n({SECOND_PAGE_TEXT}) Tj\nET\n");
    let [x0, y0, x1, y1] = rect;
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [0 0 300 300] \
         /Resources << /Font << /Helv 9 0 R >> >> /Contents 5 0 R >>"
            .to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [0 0 300 300] \
         /Resources << /Font << /Helv 9 0 R >> >> /Contents 6 0 R /Annots [7 0 R] >>"
            .to_string(),
        "<< /Length 0 >>\nstream\nendstream".to_string(),
        "<< /Length 0 >>\nstream\nendstream".to_string(),
        format!(
            "<< /Type /Annot /Subtype /FreeText /Rect [{x0} {y0} {x1} {y1}] \
             /Contents ({SECOND_PAGE_TEXT}) /DA (0 0 0 rg /Helv 14 Tf) /AP << /N 8 0 R >> >>"
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 220 40] \
             /Resources << /Font << /Helv 9 0 R >> >> /Length {} >>\nstream\n{appearance}endstream",
            appearance.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];

    assemble_pdf(&objects)
}

fn assemble_pdf(objects: &[String]) -> Vec<u8> {
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }

    let xref_offset = bytes.len();
    let size = objects.len() + 1;
    bytes.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    bytes
}

fn extraction_config() -> ExtractionConfig {
    ExtractionConfig {
        use_cache: false,
        result_format: ResultFormat::ElementBased,
        ocr: Some(OcrConfig {
            enabled: false,
            ..Default::default()
        }),
        pdf_options: Some(PdfConfig {
            extract_annotations: false,
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn should_recover_visible_annotation_text_when_native_page_body_is_empty() {
    let config = extraction_config();
    let result = extract(
        ExtractInput::from_bytes(rotated_pdf(None, AnnotationOptions::default()), PDF_MIME, None),
        &config,
    )
    .await
    .expect("annotation-only PDF extraction must succeed");
    let document = result
        .results
        .first()
        .expect("one input must yield one extracted document");

    assert_eq!(document.content, EXPECTED_TEXT);
    assert!(document.annotations.is_none());
    let fallback_warnings: Vec<_> = document
        .processing_warnings
        .iter()
        .filter(|warning| warning.source == FALLBACK_WARNING_SOURCE)
        .collect();
    assert_eq!(fallback_warnings.len(), 1);
    assert_eq!(fallback_warnings[0].message, FALLBACK_WARNING_MESSAGE);
    assert!(
        document
            .elements
            .as_deref()
            .is_some_and(|elements| elements.iter().any(|element| element.text == EXPECTED_TEXT)),
        "recovered annotation text must be represented in document elements: {:?}",
        document.elements
    );
}

#[tokio::test]
async fn should_not_include_free_text_annotation_when_native_body_is_non_empty() {
    let config = extraction_config();
    let result = extract(
        ExtractInput::from_bytes(
            rotated_pdf(Some(BODY_TEXT), AnnotationOptions::default()),
            PDF_MIME,
            None,
        ),
        &config,
    )
    .await
    .expect("PDF extraction with a native body must succeed");
    let document = result
        .results
        .first()
        .expect("one input must yield one extracted document");

    assert_eq!(document.content, BODY_TEXT);
    assert!(document.annotations.is_none());
    assert!(
        !document
            .processing_warnings
            .iter()
            .any(|warning| warning.source == FALLBACK_WARNING_SOURCE),
        "the empty-body fallback warning must not fire for a non-empty native body"
    );
}

#[tokio::test]
async fn should_not_recover_hidden_free_text_annotation_as_document_content() {
    for flags in [
        INVISIBLE_ANNOTATION_FLAG,
        HIDDEN_ANNOTATION_FLAG,
        NO_VIEW_ANNOTATION_FLAG,
    ] {
        assert_annotation_is_excluded(AnnotationOptions {
            flags,
            ..Default::default()
        })
        .await;
    }
}

#[tokio::test]
async fn should_not_recover_transparent_free_text_annotation_as_document_content() {
    assert_annotation_is_excluded(AnnotationOptions {
        opacity: Some(0.0),
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn should_not_recover_free_text_annotation_without_visible_page_area() {
    for rect in [[700.0, 100.0, 800.0, 140.0], [100.0, 100.0, 100.0, 140.0]] {
        assert_annotation_is_excluded(AnnotationOptions {
            rect,
            ..Default::default()
        })
        .await;
    }
}

#[tokio::test]
async fn should_not_recover_non_free_text_annotation_as_document_content() {
    assert_annotation_is_excluded(AnnotationOptions {
        subtype: "Text",
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn should_not_recover_free_text_annotation_on_hidden_optional_content_layer() {
    assert_annotation_is_excluded(AnnotationOptions {
        hidden_optional_content: true,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn should_recover_annotation_into_its_exact_page_and_element() {
    let config = extraction_config();
    let result = extract(
        ExtractInput::from_bytes(
            two_page_pdf_with_second_page_annotation([100.0, 100.0, 250.0, 140.0]),
            PDF_MIME,
            None,
        ),
        &config,
    )
    .await
    .expect("two-page annotation-only PDF extraction must succeed");
    let document = result.results.first().expect("one document must be returned");
    let pages = document
        .pages
        .as_deref()
        .expect("element-based extraction must return pages");
    let elements = document
        .elements
        .as_deref()
        .expect("element-based extraction must return elements");

    assert_eq!(document.content, SECOND_PAGE_TEXT);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].page_number, 1);
    assert_eq!(pages[0].content, "");
    assert_eq!(pages[0].is_blank, Some(true));
    assert_eq!(pages[1].page_number, 2);
    assert_eq!(pages[1].content, SECOND_PAGE_TEXT);
    assert_eq!(pages[1].is_blank, Some(false));
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].text, SECOND_PAGE_TEXT);
    assert_eq!(elements[0].metadata.page_number, Some(2));
    let page_structure = document
        .metadata
        .pages
        .as_ref()
        .expect("page structure must be retained");
    let boundaries = page_structure
        .boundaries
        .as_deref()
        .expect("page boundaries must be rebuilt");
    assert_eq!(boundaries.len(), 2);
    assert_eq!(boundaries[0].page_number, 1);
    assert_eq!((boundaries[0].byte_start, boundaries[0].byte_end), (0, 0));
    assert_eq!(boundaries[1].page_number, 2);
    assert_eq!(
        (boundaries[1].byte_start, boundaries[1].byte_end),
        (2, 2 + SECOND_PAGE_TEXT.len())
    );
    let metadata_pages = page_structure
        .pages
        .as_deref()
        .expect("per-page metadata must be retained");
    assert_eq!(metadata_pages[0].is_blank, Some(true));
    assert_eq!(metadata_pages[1].is_blank, Some(false));
    assert!(document.annotations.is_none());
}

#[tokio::test]
async fn should_not_recover_annotation_outside_crop_box() {
    let config = extraction_config();
    let result = extract(
        ExtractInput::from_bytes(
            two_page_pdf_with_second_page_annotation([400.0, 100.0, 500.0, 140.0]),
            PDF_MIME,
            None,
        ),
        &config,
    )
    .await
    .expect("PDF with an annotation outside CropBox must succeed");
    let document = result.results.first().expect("one document must be returned");

    assert_eq!(document.content, "");
    assert!(document.annotations.is_none());
    assert!(
        document
            .processing_warnings
            .iter()
            .all(|warning| warning.source != FALLBACK_WARNING_SOURCE)
    );
}

async fn assert_annotation_is_excluded(annotation: AnnotationOptions<'_>) {
    let config = extraction_config();
    let result = extract(
        ExtractInput::from_bytes(rotated_pdf(None, annotation), PDF_MIME, None),
        &config,
    )
    .await
    .expect("PDF extraction with an excluded annotation must succeed");
    let document = result
        .results
        .first()
        .expect("one input must yield one extracted document");

    assert_eq!(document.content, "");
    assert!(document.annotations.is_none());
    assert!(
        document
            .processing_warnings
            .iter()
            .all(|warning| warning.source != FALLBACK_WARNING_SOURCE),
        "fallback warning must not claim excluded annotation text was recovered"
    );
}
