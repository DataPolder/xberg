//! Email message extractor.

use crate::Result;
use crate::core::config::ExtractionConfig;
use crate::extractors::SyncExtractor;
use crate::extractors::security::SecurityBudget;
use crate::plugins::{InternalDocumentExtractor, Plugin};
use crate::types::internal::InternalDocument;
use crate::types::internal_builder::InternalDocumentBuilder;
use crate::types::metadata::Metadata;
use crate::types::{ArchiveEntry, EmailMetadata, ProcessingWarning};
use ahash::AHashMap;
use async_trait::async_trait;
use std::borrow::Cow;
#[cfg(feature = "tokio-runtime")]
use std::path::Path;

const EMAIL_STRUCT_KEYS: &[&str] = &[
    "from_email",
    "from_name",
    "to_emails",
    "cc_emails",
    "bcc_emails",
    "message_id",
    "attachments",
    "subject",
    "date",
    "email_from",
    "email_to",
    "email_cc",
    "email_bcc",
];
#[cfg_attr(alef, alef(skip))]
/// Email message extractor.
///
/// Supports: .eml, .msg
pub struct EmailExtractor;

impl Default for EmailExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailExtractor {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl EmailExtractor {
    /// Build an `InternalDocument` from extracted email content.
    ///
    /// Pushes email headers as a metadata block, then body content as paragraphs.
    fn build_internal_document(email_result: &crate::types::EmailExtractionResult) -> InternalDocument {
        let mut builder = InternalDocumentBuilder::new("email");

        let mut header_entries = Vec::new();
        if let Some(ref subject) = email_result.subject {
            header_entries.push(("Subject".to_string(), subject.clone()));
        }
        if let Some(from) = Self::format_sender(email_result) {
            header_entries.push(("From".to_string(), from));
        }
        if !email_result.to_emails.is_empty() {
            header_entries.push(("To".to_string(), email_result.to_emails.join(", ")));
        }
        if !email_result.cc_emails.is_empty() {
            header_entries.push(("CC".to_string(), email_result.cc_emails.join(", ")));
        }
        if let Some(ref date) = email_result.date {
            header_entries.push(("Date".to_string(), date.clone()));
        }
        if !header_entries.is_empty() {
            builder.push_metadata_block(&header_entries, None);
        }

        if let Some(ref html) = email_result.html_content {
            let html_doc = crate::extraction::html::structure::build_document_structure(html);
            for (idx, node) in html_doc.nodes.iter().enumerate() {
                if node.parent.is_none() {
                    process_node(&html_doc, idx, &mut builder);
                }
            }
        } else {
            for paragraph in email_result.content.split("\n\n") {
                let trimmed = paragraph.trim();
                if !trimmed.is_empty() {
                    builder.push_paragraph(trimmed, vec![], None, None);
                }
            }
        }

        if !email_result.attachments.is_empty() {
            builder.push_paragraph("Attachments:", vec![], None, None);
            for att in &email_result.attachments {
                let name = att.filename.as_deref().or(att.name.as_deref()).unwrap_or("unnamed");
                let size = att.size.unwrap_or(0);
                let att_text = format!("  {} ({}B)", name, size);
                builder.push_paragraph(&att_text, vec![], None, None);
            }
        }

        builder.build()
    }

    /// Preserve both parts of a mailbox while avoiding `address <address>`.
    fn format_sender(email_result: &crate::types::EmailExtractionResult) -> Option<String> {
        let name = email_result
            .metadata
            .get("from_name")
            .filter(|name| !name.trim().is_empty());
        match (name, email_result.from_email.as_ref()) {
            (Some(name), Some(address)) if !name.eq_ignore_ascii_case(address) => Some(format!("{name} <{address}>")),
            (_, Some(address)) => Some(address.clone()),
            (Some(name), None) => Some(name.clone()),
            (None, None) => None,
        }
    }

    /// Convert one parsed email into the internal representation.
    fn build_extracted_document(
        email_result: &crate::types::EmailExtractionResult,
        mime_type: &str,
        config: &ExtractionConfig,
    ) -> Result<InternalDocument> {
        let mut doc = Self::build_internal_document(email_result);
        doc.mime_type = mime_type.to_string();
        doc.metadata = Self::build_document_metadata(email_result);

        let mut budget = SecurityBudget::from_config(config);
        for elem in &doc.elements {
            budget.account_text(elem.text.len())?;
        }

        Ok(doc)
    }

    fn build_document_metadata(email_result: &crate::types::EmailExtractionResult) -> Metadata {
        let attachment_names = email_result
            .attachments
            .iter()
            .filter_map(|attachment| attachment.filename.clone().or_else(|| attachment.name.clone()))
            .collect();
        let additional = email_result
            .metadata
            .iter()
            .filter(|(key, _)| !EMAIL_STRUCT_KEYS.contains(&key.as_str()))
            .map(|(key, value)| (Cow::Owned(key.clone()), serde_json::json!(value)))
            .collect::<AHashMap<_, _>>();
        let from_name = email_result.metadata.get("from_name").cloned();
        let email_metadata = EmailMetadata {
            from_email: email_result.from_email.clone(),
            from_name: from_name.clone(),
            to_emails: email_result.to_emails.clone(),
            cc_emails: email_result.cc_emails.clone(),
            bcc_emails: email_result.bcc_emails.clone(),
            message_id: email_result.message_id.clone(),
            attachments: attachment_names,
        };

        let authors = from_name.filter(|name| !name.is_empty()).map(|name| vec![name]);

        Metadata {
            format: Some(crate::types::FormatMetadata::Email(email_metadata)),
            subject: email_result.subject.clone(),
            authors,
            created_at: email_result.date.clone(),
            additional,
            ..Default::default()
        }
    }
}

/// Recursively process a `DocumentNode` and its children into the `InternalDocumentBuilder`.
fn process_node(
    doc: &crate::types::document_structure::DocumentStructure,
    node_idx: usize,
    builder: &mut InternalDocumentBuilder,
) {
    if let Some(node) = doc.nodes.get(node_idx) {
        match &node.content {
            crate::types::NodeContent::Paragraph { text } => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    builder.push_paragraph(trimmed, node.annotations.clone(), None, None);
                }
            }
            crate::types::NodeContent::Heading { level, text } => {
                builder.push_heading(*level, text.as_str(), None, None);
            }
            crate::types::NodeContent::Title { text } => {
                builder.push_title(text.as_str(), None, None);
            }
            crate::types::NodeContent::List { ordered } => {
                builder.push_list(*ordered);
                for &child_idx in &node.children {
                    process_node(doc, child_idx.0 as usize, builder);
                }
                builder.end_list();
            }
            crate::types::NodeContent::ListItem { text } => {
                let ordered = if let Some(parent_idx) = node.parent
                    && let Some(parent) = doc.nodes.get(parent_idx.0 as usize)
                    && let crate::types::NodeContent::List { ordered } = parent.content
                {
                    ordered
                } else {
                    false
                };
                builder.push_list_item(text.as_str(), ordered, node.annotations.clone(), None, None);

                for &child_idx in &node.children {
                    process_node(doc, child_idx.0 as usize, builder);
                }
            }
            crate::types::NodeContent::Table { grid } => {
                let rows = crate::extraction::grid_flatten::flatten_positioned_cells(
                    grid.rows as usize,
                    grid.cells
                        .iter()
                        .map(|c| (c.row, c.row_span, c.col_span, c.content.clone())),
                );
                builder.push_table_from_cells(&rows, None, None);
            }
            crate::types::NodeContent::Code { text, language } => {
                builder.push_code(text.as_str(), language.as_deref(), None, None);
            }
            crate::types::NodeContent::Formula { text } => {
                builder.push_formula(text.as_str(), None, None);
            }
            crate::types::NodeContent::MetadataBlock { entries } => {
                builder.push_metadata_block(entries, None);
            }
            crate::types::NodeContent::Quote => {
                builder.push_quote_start();
                for &child_idx in &node.children {
                    process_node(doc, child_idx.0 as usize, builder);
                }
                builder.push_quote_end();
            }
            crate::types::NodeContent::Group { label, .. } => {
                builder.push_group_start(label.as_deref(), None);
                for &child_idx in &node.children {
                    process_node(doc, child_idx.0 as usize, builder);
                }
                builder.push_group_end();
            }
            crate::types::NodeContent::Admonition { kind, title } => {
                builder.push_admonition(kind, title.as_deref(), None);
                for &child_idx in &node.children {
                    process_node(doc, child_idx.0 as usize, builder);
                }
            }
            crate::types::NodeContent::Image { description, .. } => {
                let text = description.as_deref().unwrap_or("[Image]");
                builder.push_paragraph(text, vec![], None, None);
            }
            _ => {
                if let Some(text) = node.content.text() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        builder.push_paragraph(trimmed, node.annotations.clone(), None, None);
                    }
                }
            }
        }
    }
}

impl Plugin for EmailExtractor {
    fn name(&self) -> &str {
        "email-extractor"
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn initialize(&self) -> Result<()> {
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl SyncExtractor for EmailExtractor {
    fn extract_sync(&self, content: &[u8], mime_type: &str, config: &ExtractionConfig) -> Result<InternalDocument> {
        let fallback_codepage = config.email.as_ref().and_then(|e| e.msg_fallback_codepage);
        let email_result = crate::extraction::email::extract_email_content(content, mime_type, fallback_codepage)?;
        Self::build_extracted_document(&email_result, mime_type, config)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl InternalDocumentExtractor for EmailExtractor {
    async fn extract_content(
        &self,
        content: &[u8],
        mime_type: &str,
        config: &ExtractionConfig,
    ) -> Result<InternalDocument> {
        tracing::debug!(format = "email", size_bytes = content.len(), "extraction starting");
        let fallback_codepage = config.email.as_ref().and_then(|email| email.msg_fallback_codepage);
        let parsed =
            crate::extraction::email::extract_email_content_with_nested(content, mime_type, fallback_codepage)?;
        let mut doc = Self::build_extracted_document(&parsed.result, mime_type, config)?;
        let mut children = Vec::new();
        let mut warnings = Vec::new();

        if config.max_archive_depth > 0 {
            (children, warnings) = extract_attachment_children(&parsed.result.attachments, config).await;

            if mime_type == "message/rfc822" {
                let (nested_children, nested_warnings) =
                    extract_nested_message_children(&parsed.nested_messages, config).await;
                children.extend(nested_children);
                warnings.extend(nested_warnings);
            }
        }

        if !children.is_empty() {
            doc.children = Some(children);
        }
        doc.processing_warnings.extend(warnings);

        tracing::debug!(
            element_count = doc.elements.len(),
            format = "email",
            "extraction complete"
        );
        Ok(doc)
    }

    #[cfg(feature = "tokio-runtime")]
    #[cfg_attr(feature = "otel", tracing::instrument(
        skip(self, path, config),
        fields(
            extractor.name = self.name(),
        )
    ))]
    async fn extract_path(&self, path: &Path, mime_type: &str, config: &ExtractionConfig) -> Result<InternalDocument> {
        let bytes = crate::core::io::read_file_async(path).await?;
        self.extract_content(&bytes, mime_type, config).await
    }

    fn supported_mime_types(&self) -> &[&str] {
        &["message/rfc822", "application/vnd.ms-outlook"]
    }

    fn priority(&self) -> i32 {
        50
    }
}

/// Recursively extract content from email attachments.
///
/// For each attachment with binary data, detects its MIME type and dispatches
/// to the appropriate extractor. Follows the same pattern as archive extractors.
/// Errors on individual attachments are captured as warnings, never failing
/// the whole extraction.
pub(crate) async fn extract_attachment_children(
    attachments: &[crate::types::EmailAttachment],
    config: &ExtractionConfig,
) -> (Vec<ArchiveEntry>, Vec<ProcessingWarning>) {
    let mut children = Vec::new();
    let mut warnings = Vec::new();

    for (idx, attachment) in attachments.iter().enumerate() {
        let bytes = match &attachment.data {
            Some(data) if !data.is_empty() => data,
            _ => continue,
        };

        let filename = attachment
            .filename
            .clone()
            .or_else(|| attachment.name.clone())
            .unwrap_or_else(|| format!("attachment_{}", idx));

        let detected_mime = crate::core::mime::detect_mime_type_from_bytes(bytes)
            .ok()
            .or_else(|| {
                std::path::Path::new(&filename)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .and_then(|ext| mime_guess::from_ext(ext).first())
                    .map(|m| m.to_string())
            })
            .or_else(|| attachment.mime_type.clone().filter(|m| m != "application/octet-stream"));

        let file_mime = match detected_mime {
            Some(m) if m != "application/octet-stream" => m,
            _ => continue,
        };

        if config
            .max_embedded_file_bytes
            .is_some_and(|cap| bytes.len() as u64 > cap)
        {
            let cap = config.max_embedded_file_bytes.unwrap_or(0);
            warnings.push(ProcessingWarning {
                source: Cow::Borrowed("email_attachment_extraction"),
                message: Cow::Owned(format!(
                    "Skipped attachment '{}': size {} bytes exceeds cap {} bytes",
                    filename,
                    bytes.len(),
                    cap
                )),
            });
            continue;
        }

        let mut child_config = config.clone();
        child_config.max_archive_depth = config.max_archive_depth.saturating_sub(1);

        match crate::core::extractor::extract_bytes(bytes, &file_mime, &child_config).await {
            Ok(result) => {
                children.push(ArchiveEntry {
                    path: filename,
                    mime_type: file_mime,
                    result: Box::new(result),
                });
            }
            Err(e) => {
                warnings.push(ProcessingWarning {
                    source: Cow::Borrowed("email_attachment_extraction"),
                    message: Cow::Owned(format!("Failed to extract '{}': {}", filename, e)),
                });
            }
        }
    }

    (children, warnings)
}

/// Extract nested `message/rfc822` sub-messages as `ArchiveEntry` children.
///
/// Recursively extracts `PartType::Message` payloads retained by the initial
/// top-level EML parse via
/// `extract_bytes` with `message/rfc822` MIME type. This makes nested
/// messages available as separate children for recursive processing,
/// complementing the existing text-merging in `collect_nested_message_text`
/// / `collect_nested_message_html`.
async fn extract_nested_message_children(
    nested_messages: &[bytes::Bytes],
    config: &ExtractionConfig,
) -> (Vec<ArchiveEntry>, Vec<ProcessingWarning>) {
    let mut children = Vec::new();
    let mut warnings = Vec::new();

    for (nested_idx, raw_bytes) in nested_messages.iter().enumerate() {
        let filename = format!("nested_message_{nested_idx}.eml");

        if config
            .max_embedded_file_bytes
            .is_some_and(|cap| raw_bytes.len() as u64 > cap)
        {
            let cap = config.max_embedded_file_bytes.unwrap_or(0);
            warnings.push(ProcessingWarning {
                source: Cow::Borrowed("nested_message_extraction"),
                message: Cow::Owned(format!(
                    "Skipped '{}': size {} bytes exceeds cap {} bytes",
                    filename,
                    raw_bytes.len(),
                    cap
                )),
            });
            continue;
        }

        let mut child_config = config.clone();
        child_config.max_archive_depth = config.max_archive_depth.saturating_sub(1);

        match crate::core::extractor::extract_bytes(raw_bytes, "message/rfc822", &child_config).await {
            Ok(result) => {
                children.push(ArchiveEntry {
                    path: filename,
                    mime_type: "message/rfc822".to_string(),
                    result: Box::new(result),
                });
            }
            Err(e) => {
                warnings.push(ProcessingWarning {
                    source: Cow::Borrowed("nested_message_extraction"),
                    message: Cow::Owned(format!("Failed to extract '{}': {}", filename, e)),
                });
            }
        }
    }

    (children, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_extractor_plugin_interface() {
        let extractor = EmailExtractor::new();
        assert_eq!(extractor.name(), "email-extractor");
        assert!(extractor.initialize().is_ok());
        assert!(extractor.shutdown().is_ok());
    }

    #[test]
    fn test_email_extractor_supported_mime_types() {
        let extractor = EmailExtractor::new();
        let mime_types = extractor.supported_mime_types();
        assert_eq!(mime_types.len(), 2);
        assert!(mime_types.contains(&"message/rfc822"));
        assert!(mime_types.contains(&"application/vnd.ms-outlook"));
    }

    #[test]
    fn test_email_extractor_uses_config() {
        use crate::core::config::EmailConfig;

        let config = ExtractionConfig {
            email: Some(EmailConfig {
                msg_fallback_codepage: Some(1251),
            }),
            ..Default::default()
        };
        let extractor = EmailExtractor::new();
        let result = extractor.extract_sync(b"", "application/vnd.ms-outlook", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_sender_name_and_address_are_rendered_once() {
        let eml = b"From: Alice Example <alice@example.com>\r\nSubject: Sender\r\n\r\nBody";
        let doc = EmailExtractor::new()
            .extract_sync(eml, "message/rfc822", &ExtractionConfig::default())
            .unwrap();
        let from_values: Vec<&str> = doc
            .elements
            .iter()
            .filter(|element| matches!(element.kind, crate::types::internal::ElementKind::MetadataBlock))
            .flat_map(|element| element.attributes.iter())
            .flat_map(|attributes| attributes.values())
            .map(String::as_str)
            .filter(|value| value.contains("alice@example.com"))
            .collect();

        assert_eq!(from_values, vec!["Alice Example <alice@example.com>"]);
        let crate::types::FormatMetadata::Email(email) = doc.metadata.format.unwrap() else {
            panic!("expected email metadata");
        };
        assert_eq!(email.from_name.as_deref(), Some("Alice Example"));
        assert_eq!(email.from_email.as_deref(), Some("alice@example.com"));
    }

    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_async_msg_keeps_sync_output_when_extracting_attachments() {
        let data = include_bytes!("../../../../test_documents/email/attachment.msg");
        let config = ExtractionConfig {
            max_archive_depth: 1,
            ..Default::default()
        };
        let extractor = EmailExtractor::new();

        let sync_doc = extractor
            .extract_sync(data, "application/vnd.ms-outlook", &config)
            .unwrap();
        let async_doc = extractor
            .extract_content(data, "application/vnd.ms-outlook", &config)
            .await
            .unwrap();

        assert_eq!(async_doc.elements, sync_doc.elements);
        assert_eq!(
            serde_json::to_value(&async_doc.metadata).unwrap(),
            serde_json::to_value(&sync_doc.metadata).unwrap()
        );
    }

    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_async_email_preserves_extractable_attachment_children() {
        let eml = b"From: Alice <alice@example.com>\r\n\
Subject: Attachment\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=part\r\n\
\r\n\
--part\r\n\
Content-Type: text/plain\r\n\
\r\n\
Parent body\r\n\
--part\r\n\
Content-Type: text/plain\r\n\
Content-Disposition: attachment; filename=note.txt\r\n\
\r\n\
Attachment body\r\n\
--part--\r\n";
        let config = ExtractionConfig {
            max_archive_depth: 1,
            ..Default::default()
        };

        let doc = EmailExtractor::new()
            .extract_content(eml, "message/rfc822", &config)
            .await
            .unwrap();
        let children = doc.children.expect("text attachment should be extracted");

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "note.txt");
        assert!(children[0].result.content.contains("Attachment body"));
    }

    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_async_email_preserves_nested_message_child() {
        let eml = b"From: digest@example.com\r\n\
Subject: Digest\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/digest; boundary=digest\r\n\
\r\n\
--digest\r\n\
Content-Type: message/rfc822\r\n\
\r\n\
From: child@example.com\r\n\
Subject: Nested\r\n\
\r\n\
Nested body\r\n\
--digest--\r\n";
        let config = ExtractionConfig {
            max_archive_depth: 1,
            ..Default::default()
        };

        let doc = EmailExtractor::new()
            .extract_content(eml, "message/rfc822", &config)
            .await
            .unwrap();
        let children = doc.children.expect("nested message should be extracted");

        let nested = children
            .iter()
            .find(|child| child.path == "nested_message_0.eml")
            .expect("dedicated nested message child should be present");
        assert_eq!(nested.mime_type, "message/rfc822");
        assert!(nested.result.content.contains("Nested body"));
    }

    #[test]
    fn test_email_with_table_and_list_preservation() {
        let eml = r#"From: Alice <alice@example.com>
To: Bob <bob@example.com>
Subject: Table and List Repro
Content-Type: multipart/alternative; boundary="boundary"

--boundary
Content-Type: text/plain; charset=utf-8

Plain text fallback.

--boundary
Content-Type: text/html; charset=utf-8

<html>
<body>
    <p>Introduction.</p>
    <table>
        <tr><th>Name</th><th>Value</th></tr>
        <tr><td>Key</td><td>123</td></tr>
    </table>
    <ol>
        <li>First item</li>
        <li>Second item</li>
    </ol>
    <p>Conclusion.</p>
</body>
</html>
--boundary--
"#;
        let extractor = EmailExtractor::new();
        let config = ExtractionConfig::default();
        let doc = extractor
            .extract_sync(eml.as_bytes(), "message/rfc822", &config)
            .unwrap();

        let has_table = doc
            .elements
            .iter()
            .any(|e| matches!(e.kind, crate::types::internal::ElementKind::Table { .. }));
        let list_item_count = doc
            .elements
            .iter()
            .filter(|e| matches!(e.kind, crate::types::internal::ElementKind::ListItem { .. }))
            .count();
        let paragraph_count = doc
            .elements
            .iter()
            .filter(|e| matches!(e.kind, crate::types::internal::ElementKind::Paragraph))
            .count();

        assert!(has_table, "Table element should be present");
        assert_eq!(list_item_count, 2, "Should have 2 list items");
        assert_eq!(
            paragraph_count, 2,
            "Should have exactly 2 body paragraphs (no duplicates)"
        );
    }

    #[test]
    fn test_content_size_guard_fires_when_limit_exceeded() {
        use crate::extractors::security::SecurityLimits;

        let long_body = "x".repeat(1000);
        let eml = format!("From: sender@example.com\r\nSubject: Big\r\n\r\n{}", long_body);

        let config = ExtractionConfig {
            security_limits: Some(SecurityLimits {
                max_content_size: 10,
                ..SecurityLimits::default()
            }),
            ..Default::default()
        };

        let extractor = EmailExtractor::new();
        let result = extractor.extract_sync(eml.as_bytes(), "message/rfc822", &config);

        assert!(
            result.is_err(),
            "extraction must fail when content exceeds max_content_size"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.to_lowercase().contains("security") || err_msg.to_lowercase().contains("content"),
            "error must mention security or content: {}",
            err_msg
        );
    }

    #[test]
    fn test_content_size_guard_passes_for_normal_email() {
        let eml = "From: sender@example.com\r\nSubject: Hi\r\n\r\nHello world.";
        let config = ExtractionConfig::default();
        let extractor = EmailExtractor::new();
        let result = extractor.extract_sync(eml.as_bytes(), "message/rfc822", &config);
        assert!(result.is_ok(), "normal email must not be rejected: {:?}", result.err());
    }
}
