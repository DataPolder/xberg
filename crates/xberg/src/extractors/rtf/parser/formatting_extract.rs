//! The formatting-metadata pass: [`extract_rtf_formatting`] and the
//! span-to-annotation conversion consumed by callers of the text-extraction
//! pass in [`super::text_extract`].

use super::{
    FldrsltCloseState, RtfFormattingData, RtfFormattingSpan, close_fldinst_group, close_fldrslt_group,
    consume_adjacent_hex_escape, parse_font_charset_table, parse_rtf_color_table, resolve_decode_codepage,
};
use crate::extractors::rtf::encoding::{decode_ansi_bytes, parse_hex_byte, parse_rtf_control_word};
use crate::types::TextAnnotation;
use crate::types::document_structure::AnnotationKind;
use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Clone)]
struct FmtState {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    color_idx: u16,
}

fn push_span_if_open(spans: &mut Vec<RtfFormattingSpan>, span_start: &mut usize, text_offset: usize, fmt: &FmtState) {
    if text_offset > *span_start {
        spans.push(RtfFormattingSpan {
            start: *span_start,
            end: text_offset,
            bold: fmt.bold,
            italic: fmt.italic,
            underline: fmt.underline,
            strikethrough: fmt.strikethrough,
            color_index: fmt.color_idx,
        });
    }
    *span_start = text_offset;
}

struct FormattingEscapeCtx<'a> {
    spans: &'a mut Vec<RtfFormattingSpan>,
    span_start: &'a mut usize,
    text_offset: &'a mut usize,
    fmt: &'a mut FmtState,
    font_charsets: &'a HashMap<u16, u32>,
    ansi_codepage_stack: &'a mut Vec<u32>,
    font_id_stack: &'a mut Vec<Option<u16>>,
    default_font_id: &'a mut Option<u16>,
    group_has_text: &'a mut Vec<bool>,
    pending_boundary_space: &'a mut bool,
    expect_destination: &'a mut bool,
    ignorable_pending: &'a mut bool,
    group_depth: i32,
    skip_depth: &'a mut i32,
    skip_destinations: &'a [&'a str],
    in_fldinst: &'a mut bool,
    fldinst_depth: &'a mut i32,
    fldinst_content: &'a mut String,
    in_fldrslt: &'a mut bool,
    fldrslt_depth: &'a mut i32,
    fldrslt_start: &'a mut usize,
    in_header: &'a mut bool,
    header_depth: &'a mut i32,
    header_buf: &'a mut String,
    in_footer: &'a mut bool,
    footer_depth: &'a mut i32,
    footer_buf: &'a mut String,
}

impl FormattingEscapeCtx<'_> {
    fn handle(&mut self, chars: &mut Peekable<Chars<'_>>) {
        let Some(&next_ch) = chars.peek() else {
            return;
        };
        match next_ch {
            '\\' | '{' | '}' => self.handle_literal(next_ch, chars),
            '\'' => self.handle_hex_escape(chars),
            '*' => {
                chars.next();
                *self.ignorable_pending = true;
            }
            _ => self.handle_control_word(chars),
        }
    }

    fn handle_literal(&mut self, next_ch: char, chars: &mut Peekable<Chars<'_>>) {
        chars.next();
        *self.expect_destination = false;
        if *self.in_fldinst {
            self.fldinst_content.push(next_ch);
        }
        if *self.skip_depth > 0 {
            return;
        }
        self.add_boundary_space();
        *self.text_offset += next_ch.len_utf8();
        if let Some(flag) = self.group_has_text.last_mut() {
            *flag = true;
        }
        if *self.in_header {
            self.header_buf.push(next_ch);
        }
        if *self.in_footer {
            self.footer_buf.push(next_ch);
        }
    }

    fn handle_hex_escape(&mut self, chars: &mut Peekable<Chars<'_>>) {
        chars.next();
        *self.expect_destination = false;
        let bytes = parse_hex_escape_bytes(chars);
        if *self.skip_depth > 0 {
            return;
        }
        let Some(bytes) = bytes.as_deref() else {
            return;
        };
        let codepage = resolve_decode_codepage(
            self.font_id_stack,
            *self.default_font_id,
            self.font_charsets,
            self.ansi_codepage_stack,
        );
        let decoded = decode_ansi_bytes(bytes, codepage);
        self.add_boundary_space();
        *self.text_offset += decoded.len();
        if let Some(flag) = self.group_has_text.last_mut() {
            *flag = true;
        }
    }

    fn add_boundary_space(&mut self) {
        if *self.pending_boundary_space && *self.text_offset > 0 {
            *self.text_offset += 1;
        }
        *self.pending_boundary_space = false;
    }

    fn handle_control_word(&mut self, chars: &mut Peekable<Chars<'_>>) {
        let (word, param) = parse_rtf_control_word(chars);
        if *self.expect_destination || *self.ignorable_pending {
            *self.expect_destination = false;
            if self.handle_destination(&word) {
                return;
            }
        }
        if *self.in_fldinst {
            self.fldinst_content.push_str(&word);
        }
        self.update_scope(&word, param);
        if *self.skip_depth > 0 {
            return;
        }
        if self.update_format(&word, param) {
            return;
        }
        self.emit_control_word(&word, param, chars);
    }

    fn handle_destination(&mut self, word: &str) -> bool {
        if *self.ignorable_pending {
            *self.ignorable_pending = false;
            if word == "fldinst" {
                *self.in_fldinst = true;
                *self.fldinst_depth = self.group_depth;
            }
            self.start_skip();
            return true;
        }
        match word {
            "fldinst" => {
                *self.in_fldinst = true;
                *self.fldinst_depth = self.group_depth;
                self.start_skip();
            }
            "fldrslt" => {
                *self.in_fldrslt = true;
                *self.fldrslt_depth = self.group_depth;
                *self.fldrslt_start = *self.text_offset;
            }
            destination if self.skip_destinations.contains(&destination) => self.start_skip(),
            _ => return false,
        }
        true
    }

    fn start_skip(&mut self) {
        if *self.skip_depth == 0 {
            *self.skip_depth = self.group_depth;
        }
    }

    fn update_scope(&mut self, word: &str, param: Option<i32>) {
        match word {
            "ansicpg" => {
                if let Some(value) = param
                    && value > 0
                    && let Some(codepage) = self.ansi_codepage_stack.last_mut()
                {
                    *codepage = value as u32;
                }
            }
            "f" => {
                if let Some(value) = param
                    && let Some(font_id) = self.font_id_stack.last_mut()
                {
                    *font_id = Some(value.max(0) as u16);
                }
            }
            "deff" => {
                if let Some(value) = param {
                    *self.default_font_id = Some(value.max(0) as u16);
                }
            }
            _ => {}
        }
    }

    fn update_format(&mut self, word: &str, param: Option<i32>) -> bool {
        let changed = match word {
            "b" => self.fmt.bold != (param.unwrap_or(1) != 0),
            "i" => self.fmt.italic != (param.unwrap_or(1) != 0),
            "ul" => self.fmt.underline != (param.unwrap_or(1) != 0),
            "ulnone" => self.fmt.underline,
            "strike" => self.fmt.strikethrough != (param.unwrap_or(1) != 0),
            "cf" => self.fmt.color_idx != param.unwrap_or(0) as u16,
            "plain" => {
                self.fmt.bold
                    || self.fmt.italic
                    || self.fmt.underline
                    || self.fmt.strikethrough
                    || self.fmt.color_idx != 0
            }
            _ => return false,
        };
        if changed {
            push_span_if_open(self.spans, self.span_start, *self.text_offset, self.fmt);
        }
        match word {
            "b" => self.fmt.bold = param.unwrap_or(1) != 0,
            "i" => self.fmt.italic = param.unwrap_or(1) != 0,
            "ul" => self.fmt.underline = param.unwrap_or(1) != 0,
            "ulnone" => self.fmt.underline = false,
            "strike" => self.fmt.strikethrough = param.unwrap_or(1) != 0,
            "cf" => self.fmt.color_idx = param.unwrap_or(0) as u16,
            "plain" => {
                self.fmt.bold = false;
                self.fmt.italic = false;
                self.fmt.underline = false;
                self.fmt.strikethrough = false;
                self.fmt.color_idx = 0;
            }
            _ => unreachable!(),
        }
        true
    }

    fn emit_control_word(&mut self, word: &str, param: Option<i32>, chars: &mut Peekable<Chars<'_>>) {
        match word {
            "header" | "headerl" | "headerr" | "headerf" => {
                *self.in_header = true;
                *self.header_depth = self.group_depth;
            }
            "footer" | "footerl" | "footerr" | "footerf" => {
                *self.in_footer = true;
                *self.footer_depth = self.group_depth;
            }
            "par" | "line" => {
                *self.text_offset += 1;
                if *self.in_header {
                    self.header_buf.push('\n');
                }
                if *self.in_footer {
                    self.footer_buf.push('\n');
                }
            }
            "tab" => *self.text_offset += 1,
            "bullet" => *self.text_offset += '\u{2022}'.len_utf8(),
            "lquote" => *self.text_offset += '\u{2018}'.len_utf8(),
            "rquote" => *self.text_offset += '\u{2019}'.len_utf8(),
            "ldblquote" => *self.text_offset += '\u{201C}'.len_utf8(),
            "rdblquote" => *self.text_offset += '\u{201D}'.len_utf8(),
            "endash" => *self.text_offset += '\u{2013}'.len_utf8(),
            "emdash" => *self.text_offset += '\u{2014}'.len_utf8(),
            "u" => self.emit_unicode(param, chars),
            _ => {}
        }
    }

    fn emit_unicode(&mut self, param: Option<i32>, chars: &mut Peekable<Chars<'_>>) {
        if let Some(code_num) = param {
            let code_u = if code_num < 0 {
                (code_num + 65536) as u32
            } else {
                code_num as u32
            };
            if let Some(character) = char::from_u32(code_u) {
                *self.text_offset += character.len_utf8();
                if *self.in_header {
                    self.header_buf.push(character);
                }
                if *self.in_footer {
                    self.footer_buf.push(character);
                }
            }
        }
        if chars.peek().is_some_and(|next| !matches!(next, '\\' | '{' | '}')) {
            chars.next();
        }
    }
}

fn parse_hex_escape_bytes(chars: &mut Peekable<Chars<'_>>) -> Option<Vec<u8>> {
    let (Some(hex1), Some(hex2)) = (chars.next(), chars.next()) else {
        return None;
    };
    let byte = parse_hex_byte(hex1 as u8, hex2 as u8)?;
    let mut bytes = vec![byte];
    while let Some(next_byte) = consume_adjacent_hex_escape(chars) {
        bytes.push(next_byte);
    }
    Some(bytes)
}

/// Extract formatting metadata from RTF content.
///
/// This performs a lightweight pass over the RTF to extract:
/// - Bold/italic/underline formatting state changes
/// - Color table and color references
/// - Header/footer text
/// - Hyperlink field instructions
pub(crate) fn extract_rtf_formatting(content: &str) -> RtfFormattingData {
    let color_table = parse_rtf_color_table(content);
    let font_charsets = parse_font_charset_table(content);
    let mut spans = Vec::new();
    let mut hyperlinks = Vec::new();
    let mut text_offset: usize = 0;
    let mut span_start: usize = 0;

    let mut in_header = false;
    let mut in_footer = false;
    let mut header_depth: i32 = 0;
    let mut footer_depth: i32 = 0;
    let mut header_buf = String::new();
    let mut footer_buf = String::new();

    let mut in_fldinst = false;
    let mut fldinst_depth: i32 = 0;
    let mut fldinst_content = String::new();
    let mut in_fldrslt = false;
    let mut fldrslt_depth: i32 = 0;
    let mut fldrslt_start: usize = 0;
    let mut pending_hyperlink_url: Option<String> = None;

    // Closes the current formatting span (if the output advanced since
    // `span_start`) using `fmt`'s active formatting, then advances
    // `span_start` to `text_offset` unconditionally. Used on `}`, on
    // `\plain`, and by each `update_fmt_field!` invocation below -- all
    // three previously duplicated this exact push-then-advance pattern. ~keep
    let mut fmt = FmtState {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        color_idx: 0,
    };
    let mut fmt_stack: Vec<FmtState> = Vec::new();

    let mut group_depth: i32 = 0;
    let mut skip_depth: i32 = 0;
    let mut chars = content.chars().peekable();
    let mut expect_destination = false;
    let mut ignorable_pending = false;

    // Mirrors the extraction pass's codepage tracking so both passes count the
    // same number of output bytes for `\'hh` escape runs.
    let mut ansi_codepage_stack: Vec<u32> = vec![1252];
    // Mirrors the extraction pass's active-font tracking (see `font_id_stack`
    // in `extract_text_from_rtf`) so `\'hh` escapes decode identically in both.
    let mut font_id_stack: Vec<Option<u16>> = vec![None];
    let mut default_font_id: Option<u16> = None;

    let skip_dests = [
        "fonttbl",
        "stylesheet",
        "info",
        "listtable",
        "listoverridetable",
        "generator",
        "filetbl",
        "revtbl",
        "rsidtbl",
        "xmlnstbl",
        "mmathPr",
        "themedata",
        "colorschememapping",
        "datastore",
        "latentstyles",
        "datafield",
        "objdata",
        "objclass",
        "panose",
        "bkmkstart",
        "bkmkend",
        "wgrffmtfilter",
        "fcharset",
        "pgdsctbl",
        "colortbl",
        "pict",
    ];

    let mut group_has_text: Vec<bool> = Vec::new();
    let mut pending_boundary_space = false;

    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                group_depth += 1;
                expect_destination = true;
                fmt_stack.push(fmt.clone());
                group_has_text.push(false);
                pending_boundary_space = false;
                let current_codepage = ansi_codepage_stack.last().copied().unwrap_or(1252);
                ansi_codepage_stack.push(current_codepage);
                let current_font = font_id_stack.last().copied().flatten();
                font_id_stack.push(current_font);
            }
            '}' => {
                group_depth -= 1;
                expect_destination = false;
                ignorable_pending = false;
                if ansi_codepage_stack.len() > 1 {
                    ansi_codepage_stack.pop();
                }
                if font_id_stack.len() > 1 {
                    font_id_stack.pop();
                }
                if let Some(parent) = fmt_stack.pop() {
                    let changed = fmt.bold != parent.bold
                        || fmt.italic != parent.italic
                        || fmt.underline != parent.underline
                        || fmt.strikethrough != parent.strikethrough
                        || fmt.color_idx != parent.color_idx;
                    if changed {
                        push_span_if_open(&mut spans, &mut span_start, text_offset, &fmt);
                        fmt = parent;
                    }
                }
                if skip_depth > 0 && group_depth < skip_depth {
                    skip_depth = 0;
                }
                if in_header && group_depth < header_depth {
                    in_header = false;
                }
                if in_footer && group_depth < footer_depth {
                    in_footer = false;
                }
                close_fldinst_group(
                    group_depth,
                    &mut in_fldinst,
                    fldinst_depth,
                    &mut fldinst_content,
                    &mut pending_hyperlink_url,
                );
                close_fldrslt_group(
                    group_depth,
                    text_offset,
                    FldrsltCloseState {
                        in_fldrslt: &mut in_fldrslt,
                        fldrslt_depth,
                        fldrslt_start,
                        pending_hyperlink_url: &mut pending_hyperlink_url,
                        hyperlinks: &mut hyperlinks,
                    },
                );
                let produced_text = group_has_text.pop().unwrap_or(false);
                if produced_text && skip_depth == 0 {
                    pending_boundary_space = true;
                }
            }
            '\\' => {
                let mut escape_ctx = FormattingEscapeCtx {
                    spans: &mut spans,
                    span_start: &mut span_start,
                    text_offset: &mut text_offset,
                    fmt: &mut fmt,
                    font_charsets: &font_charsets,
                    ansi_codepage_stack: &mut ansi_codepage_stack,
                    font_id_stack: &mut font_id_stack,
                    default_font_id: &mut default_font_id,
                    group_has_text: &mut group_has_text,
                    pending_boundary_space: &mut pending_boundary_space,
                    expect_destination: &mut expect_destination,
                    ignorable_pending: &mut ignorable_pending,
                    group_depth,
                    skip_depth: &mut skip_depth,
                    skip_destinations: &skip_dests,
                    in_fldinst: &mut in_fldinst,
                    fldinst_depth: &mut fldinst_depth,
                    fldinst_content: &mut fldinst_content,
                    in_fldrslt: &mut in_fldrslt,
                    fldrslt_depth: &mut fldrslt_depth,
                    fldrslt_start: &mut fldrslt_start,
                    in_header: &mut in_header,
                    header_depth: &mut header_depth,
                    header_buf: &mut header_buf,
                    in_footer: &mut in_footer,
                    footer_depth: &mut footer_depth,
                    footer_buf: &mut footer_buf,
                };
                escape_ctx.handle(&mut chars);
            }
            '\n' | '\r' => {}
            ' ' | '\t' => {
                if in_fldinst {
                    fldinst_content.push(' ');
                }
                if skip_depth > 0 {
                    continue;
                }
                if text_offset > 0 {
                    text_offset += 1;
                    if let Some(flag) = group_has_text.last_mut() {
                        *flag = true;
                    }
                }
            }
            _ => {
                if in_fldinst {
                    fldinst_content.push(ch);
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }
                if pending_boundary_space && text_offset > 0 {
                    text_offset += 1;
                }
                pending_boundary_space = false;
                text_offset += ch.len_utf8();
                if let Some(flag) = group_has_text.last_mut() {
                    *flag = true;
                }
                if in_header {
                    header_buf.push(ch);
                }
                if in_footer {
                    footer_buf.push(ch);
                }
            }
        }
    }

    if text_offset > span_start && (fmt.bold || fmt.italic || fmt.underline || fmt.strikethrough || fmt.color_idx != 0)
    {
        spans.push(RtfFormattingSpan {
            start: span_start,
            end: text_offset,
            bold: fmt.bold,
            italic: fmt.italic,
            underline: fmt.underline,
            strikethrough: fmt.strikethrough,
            color_index: fmt.color_idx,
        });
    }

    let header_trimmed = header_buf.trim().to_string();
    let footer_trimmed = footer_buf.trim().to_string();

    RtfFormattingData {
        spans,
        color_table,
        header_text: if header_trimmed.is_empty() {
            None
        } else {
            Some(header_trimmed)
        },
        footer_text: if footer_trimmed.is_empty() {
            None
        } else {
            Some(footer_trimmed)
        },
        hyperlinks,
    }
}

/// Convert RTF formatting spans into `TextAnnotation` vectors for a paragraph.
///
/// Given the byte range of a paragraph within the full extracted text,
/// produces annotations from the formatting spans that overlap.
pub(crate) fn spans_to_annotations(
    para_start: usize,
    para_end: usize,
    formatting: &RtfFormattingData,
) -> Vec<TextAnnotation> {
    let mut annotations = Vec::new();
    for span in &formatting.spans {
        if span.end <= para_start || span.start >= para_end {
            continue;
        }
        let ann_start = span.start.max(para_start) - para_start;
        let ann_end = span.end.min(para_end) - para_start;
        if ann_start >= ann_end {
            continue;
        }
        let s = ann_start as u32;
        let e = ann_end as u32;
        if span.bold {
            annotations.push(TextAnnotation {
                start: s,
                end: e,
                kind: AnnotationKind::Bold,
            });
        }
        if span.italic {
            annotations.push(TextAnnotation {
                start: s,
                end: e,
                kind: AnnotationKind::Italic,
            });
        }
        if span.underline {
            annotations.push(TextAnnotation {
                start: s,
                end: e,
                kind: AnnotationKind::Underline,
            });
        }
        if span.strikethrough {
            annotations.push(TextAnnotation {
                start: s,
                end: e,
                kind: AnnotationKind::Strikethrough,
            });
        }
        if span.color_index > 0
            && let Some(color) = formatting.color_table.get(span.color_index as usize)
            && !color.is_empty()
            && color != "#000000"
        {
            annotations.push(TextAnnotation {
                start: s,
                end: e,
                kind: AnnotationKind::Color { value: color.clone() },
            });
        }
    }

    for (link_start, link_end, url) in &formatting.hyperlinks {
        if *link_end <= para_start || *link_start >= para_end {
            continue;
        }
        let s = (link_start.max(&para_start) - para_start) as u32;
        let e = (link_end.min(&para_end) - para_start) as u32;
        if s < e {
            annotations.push(TextAnnotation {
                start: s,
                end: e,
                kind: AnnotationKind::Link {
                    url: url.clone(),
                    title: None,
                },
            });
        }
    }

    annotations
}
