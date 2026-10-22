//! Email extraction functions.
//!
//! Parses .eml (RFC822) and .msg (Outlook) email files using `mail-parser`.
//! Extracts message content, headers, and attachment information.
//!
//! # Features
//!
//! - **EML support**: RFC822 format parsing
//! - **HTML to text**: Strips HTML tags from HTML email bodies
//! - **Metadata extraction**: Sender, recipients, subject, message ID
//! - **Attachments**: Lists attachment metadata and recursively extracts supported content
//!
//! # Example
//!
//! ```ignore
//! use xberg::extraction::email::parse_eml_content;
//!
//! # fn example() -> xberg::Result<()> {
//! let eml_bytes = std::fs::read("message.eml")?;
//! let result = parse_eml_content(&eml_bytes)?;
//!
//! println!("From: {:?}", result.from_email);
//! println!("Subject: {:?}", result.subject);
//! # Ok(())
//! # }
//! ```
use crate::error::{Result, XbergError};
use crate::extractors::security::{SecurityBudget, SecurityLimits};
use crate::text::utf8_validation;
use crate::text::windows_codepage::encoding_for_windows_codepage;
use crate::types::{EmailAttachment, EmailExtractionResult, ProcessingWarning};
use bytes::Bytes;
use mail_parser::MimeHeaders;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

static HTML_TAG_RE: OnceLock<Regex> = OnceLock::new();
static SCRIPT_RE: OnceLock<Regex> = OnceLock::new();
static STYLE_RE: OnceLock<Regex> = OnceLock::new();
static WHITESPACE_RE: OnceLock<Regex> = OnceLock::new();

const PID_TAG_HTML: u16 = 0x1013;
const PID_TAG_INTERNET_CODEPAGE: u16 = 0x3FDE;
const PID_TAG_MESSAGE_CODEPAGE: u16 = 0x3FFD;

/// PR_ATTACH_METHOD (PidTagAttachMethod): how an attachment's data is stored.
const PID_TAG_ATTACH_METHOD: u16 = 0x3705;
/// PR_ATTACH_DATA_OBJECT (PidTagAttachDataObject): for `afEmbeddedMessage`
/// attachments, this property is PT_OBJECT (type `000D`) and names a nested
/// CFB *storage* (not a stream) holding the embedded message's own properties.
const PID_TAG_ATTACH_DATA_OBJECT: u16 = 0x3701;
/// `afEmbeddedMessage`: the attachment is itself a Message object stored as a
/// nested CFB storage rather than binary stream data.
const ATTACH_METHOD_EMBEDDED_MSG: u32 = 5;

fn html_tag_regex() -> &'static Regex {
    HTML_TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").unwrap())
}

fn script_regex() -> &'static Regex {
    SCRIPT_RE.get_or_init(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap())
}

fn style_regex() -> &'static Regex {
    STYLE_RE.get_or_init(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap())
}

fn whitespace_regex() -> &'static Regex {
    WHITESPACE_RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Detect UTF-16 encoding (with or without BOM) and transcode to UTF-8 if needed.
///
/// `mail_parser` expects ASCII/UTF-8 input. If the EML file is encoded as
/// UTF-16, we transcode it to UTF-8 first.
///
/// Detection strategy:
/// 1. Check for BOM (`FF FE` = LE, `FE FF` = BE)
/// 2. If no BOM, use heuristic: EML files start with ASCII headers, so
///    alternating zero bytes indicate UTF-16 encoding.
fn maybe_transcode_utf16(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 {
        return None;
    }

    let (is_le, skip) = if data[0] == 0xFF && data[1] == 0xFE {
        (true, 2)
    } else if data[0] == 0xFE && data[1] == 0xFF {
        (false, 2)
    } else if data.len() >= 16 {
        let is_le_heuristic = data[1] == 0x00 && data[3] == 0x00 && data[5] == 0x00 && data[7] == 0x00;
        let is_be_heuristic = data[0] == 0x00 && data[2] == 0x00 && data[4] == 0x00 && data[6] == 0x00;

        if is_le_heuristic || is_be_heuristic {
            let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
            detector.feed(data, true);
            let guess = detector.guess(None, chardetng::Utf8Detection::Allow);
            if guess.name() == "UTF-8" || guess.name() == "windows-1252" {
                (is_le_heuristic, 0)
            } else {
                return None;
            }
        } else {
            return None;
        }
    } else {
        return None;
    };

    let payload = &data[skip..];
    let even_len = payload.len() & !1;
    let u16_iter = (0..even_len).step_by(2).map(|i| {
        if is_le {
            u16::from_le_bytes([payload[i], payload[i + 1]])
        } else {
            u16::from_be_bytes([payload[i], payload[i + 1]])
        }
    });

    match String::from_utf16(&u16_iter.collect::<Vec<u16>>()) {
        Ok(s) => Some(s.into_bytes()),
        Err(_) => None,
    }
}

/// Extract email content from either .eml or .msg format
pub(crate) fn extract_email_content(
    data: &[u8],
    mime_type: &str,
    fallback_codepage: Option<u32>,
) -> Result<EmailExtractionResult> {
    if data.is_empty() {
        return Err(XbergError::validation("Email content is empty".to_string()));
    }

    match mime_type {
        "message/rfc822" | "text/plain" => parse_eml_content(data),
        "application/vnd.ms-outlook" => parse_msg_content(data, fallback_codepage),
        _ => Err(XbergError::validation(format!(
            "Unsupported email MIME type: {}",
            mime_type
        ))),
    }
}

pub(crate) fn extract_email_content_with_nested(
    data: &[u8],
    mime_type: &str,
    fallback_codepage: Option<u32>,
    max_nested_message_bytes: Option<u64>,
    security_limits: &SecurityLimits,
) -> Result<ParsedEmailContent> {
    if data.is_empty() {
        return Err(XbergError::validation("Email content is empty".to_string()));
    }

    match mime_type {
        "message/rfc822" | "text/plain" => parse_eml_content_internal(data, true, max_nested_message_bytes),
        "application/vnd.ms-outlook" => {
            let (result, nested_embedded_messages, warnings) =
                parse_msg_content_with_nested(data, fallback_codepage, security_limits)?;
            Ok(ParsedEmailContent {
                result,
                nested_messages: Vec::new(),
                nested_embedded_messages,
                warnings,
            })
        }
        _ => Err(XbergError::validation(format!(
            "Unsupported email MIME type: {}",
            mime_type
        ))),
    }
}

/// DataPolder X46 (#3390): an e-mail's attachments **with their bytes**, for a
/// caller that parses each attachment itself.
///
/// The extractor already holds every attachment's bytes, but only hands back
/// what it extracted from them (`ExtractedDocument::children`); the parse that
/// keeps the bytes is crate-private. This exposes it, without extracting
/// anything: the same parse as [`crate::extractors::email`] with the same
/// configuration (`email.msg_fallback_codepage`, `max_embedded_file_bytes`,
/// `security_limits`), so the attachments, their names and their order are the
/// ones the extractor's children come from.
#[derive(Debug, Clone, Default)]
pub struct EmailAttachments {
    /// The file attachments, bytes in [`EmailAttachment::data`], in the order
    /// of the extractor's attachment children (`message/rfc822` parts left
    /// out, as there). An `.eml`'s nested messages follow, one per part, as
    /// `nested_message_{n}.eml` with mime `message/rfc822` and their raw bytes
    /// — the names the extractor gives those children. A part over
    /// `max_embedded_file_bytes` carries no bytes and is left out.
    pub attachments: Vec<EmailAttachment>,
    /// A `.msg`'s embedded messages (`afEmbeddedMessage`), parsed, flat across
    /// every nesting depth, in the extractor's order; each carries its own
    /// attachments with their bytes. Empty for an `.eml`.
    pub embedded_messages: Vec<EmailExtractionResult>,
}

/// See [`EmailAttachments`]. `mime_type` as for [`extract_email_content`]:
/// `message/rfc822` (or `text/plain`) for an `.eml`,
/// `application/vnd.ms-outlook` for a `.msg`. With `max_archive_depth == 0`
/// no nested message is returned, as the extractor returns no children then.
pub fn email_attachments(
    content: &[u8],
    mime_type: &str,
    config: &crate::core::config::ExtractionConfig,
) -> Result<EmailAttachments> {
    let fallback_codepage = config.email.as_ref().and_then(|email| email.msg_fallback_codepage);
    let parsed = if config.max_archive_depth > 0 {
        let security_limits = config.security_limits.clone().unwrap_or_default();
        extract_email_content_with_nested(
            content,
            mime_type,
            fallback_codepage,
            config.max_embedded_file_bytes,
            &security_limits,
        )?
    } else {
        ParsedEmailContent {
            result: extract_email_content(content, mime_type, fallback_codepage)?,
            nested_messages: Vec::new(),
            nested_embedded_messages: Vec::new(),
            warnings: Vec::new(),
        }
    };
    let mut attachments: Vec<EmailAttachment> = parsed
        .result
        .attachments
        .into_iter()
        .filter(|a| {
            !a.mime_type
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case("message/rfc822"))
        })
        .collect();
    for (idx, nested) in parsed.nested_messages.iter().enumerate() {
        if let Some(data) = &nested.data {
            attachments.push(EmailAttachment {
                name: None,
                filename: Some(format!("nested_message_{idx}.eml")),
                mime_type: Some("message/rfc822".to_string()),
                size: Some(nested.size),
                is_image: false,
                data: Some(data.clone()),
            });
        }
    }
    Ok(EmailAttachments {
        attachments,
        embedded_messages: parsed.nested_embedded_messages,
    })
}

/// Build text output from email extraction result
///
/// Renders Subject/From/To/CC/BCC/Date plus, when present, Reply-To, Message-ID,
/// In-Reply-To, References, List-Id, and List-Unsubscribe. Headers with no value
/// are omitted rather than rendered as empty lines.
pub(crate) fn build_email_text_output(result: &EmailExtractionResult) -> String {
    let mut text_parts = Vec::with_capacity(16);

    if let Some(ref subject) = result.subject {
        text_parts.push(format!("Subject: {}", subject));
    }

    if let Some(ref from) = result.from_email {
        text_parts.push(format!("From: {}", from));
    }

    if !result.to_emails.is_empty() {
        text_parts.push(format!("To: {}", result.to_emails.join(", ")));
    }

    if !result.cc_emails.is_empty() {
        text_parts.push(format!("CC: {}", result.cc_emails.join(", ")));
    }

    if !result.bcc_emails.is_empty() {
        text_parts.push(format!("BCC: {}", result.bcc_emails.join(", ")));
    }

    if let Some(reply_to) = result.metadata.get("reply_to") {
        text_parts.push(format!("Reply-To: {}", reply_to));
    }

    if let Some(ref date) = result.date {
        text_parts.push(format!("Date: {}", date));
    }

    if let Some(ref message_id) = result.message_id {
        text_parts.push(format!("Message-ID: {}", message_id));
    }

    if let Some(in_reply_to) = result.metadata.get("in_reply_to") {
        text_parts.push(format!("In-Reply-To: {}", in_reply_to));
    }

    if let Some(references) = result.metadata.get("references") {
        text_parts.push(format!("References: {}", references));
    }

    if let Some(list_id) = result.metadata.get("list_id") {
        text_parts.push(format!("List-Id: {}", list_id));
    }

    if let Some(list_unsubscribe) = result.metadata.get("list_unsubscribe") {
        text_parts.push(format!("List-Unsubscribe: {}", list_unsubscribe));
    }

    text_parts.push(result.content.clone());

    text_parts.join("\n")
}

pub(crate) fn clean_html_content(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }

    #[cfg(feature = "html")]
    {
        if let Ok(text) = crate::extraction::html::convert_html_to_markdown(
            html,
            None,
            Some(crate::core::config::OutputFormat::Plain),
        ) {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }

    let cleaned = script_regex().replace_all(html, "");
    let cleaned = style_regex().replace_all(&cleaned, "");
    let cleaned = html_tag_regex().replace_all(&cleaned, "");
    let cleaned = whitespace_regex().replace_all(&cleaned, " ");

    cleaned.trim().to_string()
}

fn is_image_mime_type(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
}

fn parse_content_type(content_type: &str) -> String {
    let trimmed = content_type.trim();
    if trimmed.is_empty() {
        return "application/octet-stream".to_string();
    }
    trimmed
        .split(';')
        .next()
        .unwrap_or("application/octet-stream")
        .trim()
        .to_lowercase()
}

#[allow(clippy::too_many_arguments)]
/// Fields [`build_metadata`] turns into the email metadata map. Bundled purely to keep
/// that function's parameter list manageable -- each field is otherwise independent.
struct EmailMetadataFields<'a> {
    subject: &'a Option<String>,
    from_email: &'a Option<String>,
    to_emails: &'a [String],
    cc_emails: &'a [String],
    bcc_emails: &'a [String],
    date: &'a Option<String>,
    message_id: &'a Option<String>,
    attachments: &'a [EmailAttachment],
}

fn build_metadata(fields: EmailMetadataFields) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    if let Some(subj) = fields.subject {
        metadata.insert("subject".to_string(), subj.clone());
    }
    if let Some(from) = fields.from_email {
        metadata.insert("email_from".to_string(), from.clone());
    }
    if !fields.to_emails.is_empty() {
        metadata.insert("email_to".to_string(), fields.to_emails.join(", "));
    }
    if !fields.cc_emails.is_empty() {
        metadata.insert("email_cc".to_string(), fields.cc_emails.join(", "));
    }
    if !fields.bcc_emails.is_empty() {
        metadata.insert("email_bcc".to_string(), fields.bcc_emails.join(", "));
    }
    if let Some(dt) = fields.date {
        metadata.insert("date".to_string(), dt.clone());
    }
    if let Some(msg_id) = fields.message_id {
        metadata.insert("message_id".to_string(), msg_id.clone());
    }

    if !fields.attachments.is_empty() {
        let attachment_names: Vec<String> = fields
            .attachments
            .iter()
            .filter_map(|att| att.name.as_ref().or(att.filename.as_ref()))
            .cloned()
            .collect();
        if !attachment_names.is_empty() {
            metadata.insert("attachments".to_string(), attachment_names.join(", "));
        }
    }

    metadata
}

mod eml;
mod msg;
mod msg_props;
mod rtf;

// Re-exported at the crate-visible `extraction::email::*` path: `eml`, `msg` and `rtf` are
// private submodules, so a `pub(crate)` item declared inside one is otherwise unreachable
// from outside `extraction::email` (module-tree privacy gates the path, not just the leaf
// item). `extractors/email.rs` and `extractors/pst.rs` depend on these names. ~keep
use eml::parse_eml_content_internal;
pub(crate) use eml::{NestedMessagePayload, ParsedEmailContent, parse_eml_content};
use msg::{parse_msg_content, parse_msg_content_with_nested};
pub(crate) use rtf::{decompress_rtf_compressed, strip_rtf_to_plain_text};

#[cfg(test)]
mod tests;
