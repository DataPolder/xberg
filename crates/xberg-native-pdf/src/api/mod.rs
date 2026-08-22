//! High-level PDF API for simple document creation and manipulation.
//!
//! This module provides a unified, easy-to-use API for common PDF operations:
//! - Creating PDFs from Markdown, HTML, or plain text
//! - Opening and editing existing PDFs with DOM-like navigation
//! - Querying and modifying document content
//! - Converting between formats
//!
//! ## Quick Start - Creating PDFs
//!
//! ```ignore
//! use xberg_native_pdf::api::Pdf;
//!
//! // Create from Markdown
//! let mut pdf = Pdf::from_markdown("# Hello World\n\nThis is a PDF.")?;
//! pdf.save("output.pdf")?;
//!
//! // Create from HTML
//! let mut pdf = Pdf::from_html("<h1>Hello</h1><p>World</p>")?;
//! pdf.save("output.pdf")?;
//! ```
//!
//! ## Opening and Editing PDFs with DOM Access
//!
//! ```ignore
//! use xberg_native_pdf::api::Pdf;
//!
//! // Open existing PDF
//! let mut doc = Pdf::open("input.pdf")?;
//!
//! // Get a page for DOM-like navigation
//! let page = doc.page(0)?;
//!
//! // Query elements
//! for text in page.find_text_containing("Hello") {
//!     println!("Found: {} at {:?}", text.text(), text.bbox());
//! }
//!
//! // Modify content
//! let mut page = doc.page(0)?;
//! let texts = page.find_text_containing("old");
//! for t in &texts {
//!     page.set_text(t.id(), "new")?;
//! }
//! doc.save_page(page)?;
//!
//! // Navigate DOM tree
//! let page = doc.page(0)?;
//! for element in page.children() {
//!     match element {
//!         PdfElement::Text(t) => println!("Text: {}", t.text()),
//!         PdfElement::Image(i) => println!("Image: {}x{}", i.width(), i.height()),
//!         _ => {}
//!     }
//! }
//!
//! // Save modifications
//! doc.save("output.pdf")?;
//! ```
//!
//! ## Builder Pattern
//!
//! For more control over PDF creation:
//!
//! ```ignore
//! use xberg_native_pdf::api::PdfBuilder;
//! use xberg_native_pdf::writer::PageSize;
//!
//! let mut pdf = PdfBuilder::new()
//!     .title("My Document")
//!     .author("John Doe")
//!     .page_size(PageSize::A4)
//!     .margins(72.0, 72.0, 72.0, 72.0)
//!     .from_markdown("# Content")?;
//! pdf.save("output.pdf")?;
//! ```

mod pdf_builder;

pub use pdf_builder::{Pdf, PdfBuilder, PdfConfig};

pub use crate::editor::{
    AnnotationId, AnnotationWrapper, ElementId, ImageInfo, PdfElement, PdfImage, PdfPage, PdfPath, PdfStructure,
    PdfTable, PdfText,
};

pub use crate::editor::{EncryptionAlgorithm, EncryptionConfig, Permissions};

pub use crate::writer::{
    Annotation, CaretAnnotation, FileAttachmentAnnotation, FreeTextAnnotation, HighlightMode, InkAnnotation,
    LineAnnotation, LinkAction, LinkAnnotation, PolygonAnnotation, PopupAnnotation, RedactAnnotation, ShapeAnnotation,
    StampAnnotation, StampType, TextAnnotation, TextMarkupAnnotation, WatermarkAnnotation,
};

pub use crate::document::ReadingOrder;

pub use crate::geometry::Rect;

pub use crate::elements::{ImageContent, PathContent, TableContent, TextContent};

pub use crate::extractors::page_labels::{PageLabelExtractor, PageLabelRange, PageLabelStyle};
pub use crate::writer::PageLabelsBuilder;

pub use crate::extractors::xmp::{XmpExtractor, XmpMetadata};
pub use crate::writer::{XmpWriter, iso_timestamp};

pub use crate::search::{SearchOptions, SearchResult, TextSearcher};

pub use crate::writer::{AFRelationship, EmbeddedFile, EmbeddedFilesBuilder};

pub use crate::rendering::{ImageFormat, PageRenderer, RenderOptions, RenderedImage};

pub use crate::xfa::{
    ConvertedField, ConvertedPage, XfaAnalysis, XfaConversionOptions, XfaConversionResult, XfaConverter, XfaExtractor,
    XfaField, XfaFieldType, XfaForm, XfaOption, XfaPage, XfaParser, add_converted_field, add_converted_page,
    analyze_xfa_document, convert_xfa_document,
};

/// Merge multiple PDF files into a single PDF.
///
/// Returns the merged PDF as bytes.
#[cfg(not(target_arch = "wasm32"))]
pub fn merge_pdfs<P: AsRef<std::path::Path>>(paths: &[P]) -> crate::error::Result<Vec<u8>> {
    if paths.is_empty() {
        return Err(crate::error::Error::InvalidPdf("No PDF paths provided".to_string()));
    }
    let first = crate::document::PdfDocument::open(paths[0].as_ref())?;
    let mut editor = crate::editor::DocumentEditor::from_document(first)?;
    for path in &paths[1..] {
        editor.merge_from(path)?;
    }
    editor.save_to_bytes()
}
