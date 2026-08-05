//! Annotation extraction using the pdf_oxide backend.
//!
//! Maps pdf_oxide's `Annotation` types to Xberg's `PdfAnnotation` model,
//! extracting content text, bounding boxes, and link URIs.

use super::OxideDocument;
use crate::types::{BoundingBox, PdfAnnotation, PdfAnnotationType, ProcessingWarning};

/// Extract annotations from all pages of a PDF document using pdf_oxide.
///
/// Iterates over every page and every annotation on each page, mapping
/// pdf_oxide annotation subtypes to [`PdfAnnotationType`] and collecting
/// content text and bounding boxes where available.
///
/// Widget (form field) and Popup annotations are skipped as they are not
/// user-facing content annotations.
///
/// # Arguments
///
/// * `doc` - Mutable reference to the oxide document
///
/// # Returns
///
/// A `Vec<PdfAnnotation>` containing all successfully extracted annotations, and a
/// `Vec<ProcessingWarning>` describing any pages whose annotations could not be
/// read (issue #72). When the document's page count itself cannot be determined,
/// annotations are empty and a single warning is returned.
pub(crate) fn extract_annotations(doc: &mut OxideDocument) -> (Vec<PdfAnnotation>, Vec<ProcessingWarning>) {
    let page_count = match doc.doc.page_count() {
        Ok(count) => count,
        Err(e) => {
            tracing::debug!("pdf_oxide: failed to get page count for annotations: {e}");
            return (Vec::new(), vec![page_count_failure_warning(&e)]);
        }
    };

    let mut annotations = Vec::new();
    let mut warnings = Vec::new();

    for page_index in 0..page_count {
        let page_number = (page_index + 1) as u32;

        let page_annotations = match doc.doc.get_annotations(page_index) {
            Ok(annots) => annots,
            Err(e) => {
                tracing::debug!(page = page_index, "pdf_oxide: failed to get annotations: {e}");
                warnings.push(page_annotations_failure_warning(page_number, &e));
                continue;
            }
        };

        for annot in page_annotations {
            if matches!(
                annot.subtype_enum,
                pdf_oxide::AnnotationSubtype::Widget | pdf_oxide::AnnotationSubtype::Popup
            ) {
                continue;
            }

            let annotation_type = map_annotation_subtype(annot.subtype_enum);

            let content = extract_annotation_content(&annot);

            let bounding_box = annot.rect.map(|rect| BoundingBox {
                x0: rect[0],
                y0: rect[1],
                x1: rect[2],
                y1: rect[3],
            });

            annotations.push(PdfAnnotation {
                annotation_type,
                content,
                page_number,
                bounding_box,
            });
        }
    }

    (annotations, warnings)
}

/// Build the warning for issue #72's document-wide failure mode: the page count
/// itself could not be determined, so no page could even be attempted.
fn page_count_failure_warning(error: &pdf_oxide::Error) -> ProcessingWarning {
    ProcessingWarning {
        source: std::borrow::Cow::Borrowed("pdf_annotations"),
        message: std::borrow::Cow::Owned(format!(
            "annotation extraction failed: could not determine page count ({error}); no annotations were extracted"
        )),
    }
}

/// Build the warning for issue #72's per-page failure mode: annotations on one
/// page could not be read, but the rest of the document is still processed.
fn page_annotations_failure_warning(page_number: u32, error: &pdf_oxide::Error) -> ProcessingWarning {
    ProcessingWarning {
        source: std::borrow::Cow::Borrowed("pdf_annotations"),
        message: std::borrow::Cow::Owned(format!(
            "annotation extraction failed for page {page_number}: {error}; annotations on this page were skipped"
        )),
    }
}

/// Map a pdf_oxide annotation subtype to Xberg's `PdfAnnotationType`.
fn map_annotation_subtype(subtype: pdf_oxide::AnnotationSubtype) -> PdfAnnotationType {
    match subtype {
        pdf_oxide::AnnotationSubtype::Text | pdf_oxide::AnnotationSubtype::FreeText => PdfAnnotationType::Text,
        pdf_oxide::AnnotationSubtype::Highlight => PdfAnnotationType::Highlight,
        pdf_oxide::AnnotationSubtype::Link => PdfAnnotationType::Link,
        pdf_oxide::AnnotationSubtype::Stamp => PdfAnnotationType::Stamp,
        pdf_oxide::AnnotationSubtype::Underline => PdfAnnotationType::Underline,
        pdf_oxide::AnnotationSubtype::StrikeOut => PdfAnnotationType::StrikeOut,
        _ => PdfAnnotationType::Other,
    }
}

/// Extract content text from a pdf_oxide annotation.
///
/// For Link annotations, attempts to retrieve the URI from the associated
/// action. Falls back to the generic `contents` field for all types.
fn extract_annotation_content(annot: &pdf_oxide::Annotation) -> Option<String> {
    if annot.subtype_enum == pdf_oxide::AnnotationSubtype::Link
        && let Some(ref action) = annot.action
    {
        match action {
            pdf_oxide::LinkAction::Uri(uri) if !uri.is_empty() => {
                return Some(uri.clone());
            }
            _ => {}
        }
    }

    annot.contents.as_ref().filter(|s| !s.is_empty()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #72: a document-wide page-count failure must produce a
    /// `ProcessingWarning` (not just a `tracing::debug!` line the caller can
    /// never see) naming the root cause.
    #[test]
    fn test_page_count_failure_warning_names_root_cause() {
        let error = pdf_oxide::Error::InvalidPdf("corrupt xref".to_string());
        let warning = page_count_failure_warning(&error);

        assert_eq!(warning.source.as_ref(), "pdf_annotations");
        assert_eq!(
            warning.message.as_ref(),
            "annotation extraction failed: could not determine page count (Invalid PDF: corrupt xref); \
             no annotations were extracted"
        );
    }

    /// Issue #72: a single page's annotation-read failure must produce a
    /// `ProcessingWarning` naming that page, while extraction continues.
    #[test]
    fn test_page_annotations_failure_warning_names_page() {
        let error = pdf_oxide::Error::InvalidPdf("malformed /Annots array".to_string());
        let warning = page_annotations_failure_warning(3, &error);

        assert_eq!(warning.source.as_ref(), "pdf_annotations");
        assert_eq!(
            warning.message.as_ref(),
            "annotation extraction failed for page 3: Invalid PDF: malformed /Annots array; \
             annotations on this page were skipped"
        );
    }

    #[test]
    fn test_map_annotation_subtype_text() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Text),
            PdfAnnotationType::Text
        );
    }

    #[test]
    fn test_map_annotation_subtype_free_text() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::FreeText),
            PdfAnnotationType::Text
        );
    }

    #[test]
    fn test_map_annotation_subtype_highlight() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Highlight),
            PdfAnnotationType::Highlight
        );
    }

    #[test]
    fn test_map_annotation_subtype_link() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Link),
            PdfAnnotationType::Link
        );
    }

    #[test]
    fn test_map_annotation_subtype_stamp() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Stamp),
            PdfAnnotationType::Stamp
        );
    }

    #[test]
    fn test_map_annotation_subtype_underline() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Underline),
            PdfAnnotationType::Underline
        );
    }

    #[test]
    fn test_map_annotation_subtype_strikeout() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::StrikeOut),
            PdfAnnotationType::StrikeOut
        );
    }

    #[test]
    fn test_map_annotation_subtype_other() {
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Ink),
            PdfAnnotationType::Other
        );
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Circle),
            PdfAnnotationType::Other
        );
        assert_eq!(
            map_annotation_subtype(pdf_oxide::AnnotationSubtype::Square),
            PdfAnnotationType::Other
        );
    }

    #[test]
    fn test_extract_annotation_content_uri() {
        let annot = pdf_oxide::Annotation {
            annotation_type: "Annot".to_string(),
            subtype: Some("Link".to_string()),
            subtype_enum: pdf_oxide::AnnotationSubtype::Link,
            contents: None,
            rect: None,
            author: None,
            creation_date: None,
            modification_date: None,
            subject: None,
            destination: None,
            action: Some(pdf_oxide::LinkAction::Uri("https://example.com".to_string())),
            quad_points: None,
            color: None,
            opacity: None,
            flags: pdf_oxide::AnnotationFlags::empty(),
            border: None,
            interior_color: None,
            field_type: None,
            field_name: None,
            field_value: None,
            default_value: None,
            field_flags: None,
            options: None,
            appearance_state: None,
            raw_dict: None,
        };

        let content = extract_annotation_content(&annot);
        assert_eq!(content, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_extract_annotation_content_fallback() {
        let annot = pdf_oxide::Annotation {
            annotation_type: "Annot".to_string(),
            subtype: Some("Text".to_string()),
            subtype_enum: pdf_oxide::AnnotationSubtype::Text,
            contents: Some("A note".to_string()),
            rect: None,
            author: None,
            creation_date: None,
            modification_date: None,
            subject: None,
            destination: None,
            action: None,
            quad_points: None,
            color: None,
            opacity: None,
            flags: pdf_oxide::AnnotationFlags::empty(),
            border: None,
            interior_color: None,
            field_type: None,
            field_name: None,
            field_value: None,
            default_value: None,
            field_flags: None,
            options: None,
            appearance_state: None,
            raw_dict: None,
        };

        let content = extract_annotation_content(&annot);
        assert_eq!(content, Some("A note".to_string()));
    }
}
