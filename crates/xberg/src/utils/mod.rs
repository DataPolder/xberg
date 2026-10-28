//! Text utility functions for quality processing and string manipulation.
//!
//! This module provides:
//! - Quality processing: clean OCR artifacts, calculate quality scores
//! - String utilities: safe decoding, mojibake fixing, encoding detection

#[cfg(feature = "quality")]
/// OCR quality analysis helpers (artifact detection, scoring, confidence aggregation).
pub mod quality;

#[cfg(feature = "quality")]
/// String utilities: mojibake repair, encoding detection, safe truncation.
pub mod string_utils;

/// Read a `test_documents/` fixture at runtime for a `#[cfg(test)]` test.
///
/// `test_documents/` is a bucket-fetched corpus (see
/// `test_documents/scripts/fetch_corpus.py`) that is not committed, so tests must
/// not bake fixture bytes in at compile time via `include_bytes!` — that turns a
/// missing fixture into a build failure for the whole crate, which is what broke
/// the workspace-wide clippy run in CI.
///
/// `relative` is a path under `test_documents/`, e.g. `"images/test.heic"`.
/// Returns `None` — after printing a greppable skip message naming the missing
/// path — when the fixture has not been fetched. Callers must skip cleanly; a
/// `None` is "not run", never "passed".
///
/// Lives here rather than beside any one caller because `utils` is compiled
/// unconditionally: `extraction::image` is gated on the OCR features and
/// `extraction::email` on `email`, so a helper in either is invisible to the
/// other under a narrow feature set.
#[cfg(test)]
// Every caller sits behind a feature gate (`ocr`/`ocr-wasm`/`ocr-pipeline` for the image and
// heif tests, `email` for the message ones), so a build that enables none of them compiles this
// with no users at all and `-D warnings` rejects it -- which is what broke the
// `--no-default-features --features layout-tract` clippy leg. Enumerating the gates here instead
// would have to be kept in step with every future caller; allowing the unused case would not. ~keep
#[allow(dead_code)]
pub(crate) fn read_test_fixture(relative: &str) -> Option<Vec<u8>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_documents")
        .join(relative);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            eprintln!(
                "SKIP: fixture {} not available ({e}); run `python3 test_documents/scripts/fetch_corpus.py` to fetch it",
                path.display()
            );
            None
        }
    }
}

/// Strip a leading Unicode byte-order mark (U+FEFF) if present.
///
/// `encoding_rs` removes a BOM when it drives the encoding choice, but detected
/// / fast-path decodes can leave it in, where it pollutes the first token (e.g.
/// an Excel-exported UTF-8 CSV's first header cell — xberg-io/xberg#1223).
pub(crate) fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

/// The UTF-16 encoding of a text document, from its first bytes, with the length of
/// the byte order mark to skip (DataPolder #3539, X54).
///
/// Windows tools, logger software and PowerShell write `.txt` / `.csv` exports as
/// UTF-16, usually little-endian with a BOM. Neither text decoder recognised it: a
/// lossy UTF-8 decode turns the BOM into U+FFFD and keeps every second byte, the
/// zero, so `Analog` reads `A\0n\0a\0l\0o\0g\0`, splitting every word into single
/// letters and doubling the text; without a BOM UTF-16 ASCII is even valid UTF-8 and
/// passes the fast path unchanged. Recognised here:
///
/// - a UTF-16LE (`FF FE`) or UTF-16BE (`FE FF`) byte order mark, but not the UTF-32LE
///   one (`FF FE 00 00`), which is left as it was;
/// - without a BOM, the zero-byte pattern of UTF-16 in mostly-ASCII text: within the
///   first 4 KiB at least 90 % of the high bytes are zero and at most 1 % of the low
///   bytes are. Text in any 8-bit encoding has no zero bytes, so it never matches; nor
///   does UTF-16 without a BOM that is mostly outside Latin (it stays as it was).
///
/// A UTF-8 BOM is not handled here: UTF-8 decodes and the BOM is stripped as before.
/// [`xml_utils`] keeps its own sniff, which also reads XML's `<?` signature.
pub(crate) fn utf16_text_encoding(bytes: &[u8]) -> Option<(&'static encoding_rs::Encoding, usize)> {
    match bytes {
        [0xFF, 0xFE, 0x00, 0x00, ..] => return None,
        [0xFF, 0xFE, ..] => return Some((encoding_rs::UTF_16LE, 2)),
        [0xFE, 0xFF, ..] => return Some((encoding_rs::UTF_16BE, 2)),
        _ => {}
    }
    const SAMPLE: usize = 4096;
    const MIN_PAIRS: usize = 8;
    let sample = &bytes[..bytes.len().min(SAMPLE) & !1];
    let pairs = sample.len() / 2;
    if pairs < MIN_PAIRS {
        return None;
    }
    let (mut first_zero, mut second_zero) = (0usize, 0usize);
    for pair in sample.chunks_exact(2) {
        first_zero += usize::from(pair[0] == 0);
        second_zero += usize::from(pair[1] == 0);
    }
    let mostly = |n: usize| n * 10 >= pairs * 9;
    let hardly = |n: usize| n * 100 <= pairs;
    if mostly(second_zero) && hardly(first_zero) {
        Some((encoding_rs::UTF_16LE, 0))
    } else if mostly(first_zero) && hardly(second_zero) {
        Some((encoding_rs::UTF_16BE, 0))
    } else {
        None
    }
}

/// [`utf16_text_encoding`]'s decode, as a [`DecodeOutcome`]: a non-UTF-8 encoding
/// was used (`fell_back`), and an unpaired surrogate is a replacement.
fn decode_utf16_text(byte_data: &[u8]) -> Option<DecodeOutcome> {
    let (encoding, bom_len) = utf16_text_encoding(byte_data)?;
    let (decoded, had_errors) = encoding.decode_without_bom_handling(&byte_data[bom_len..]);
    Some(DecodeOutcome {
        text: decoded.into_owned(),
        fell_back: true,
        replaced_characters: had_errors,
    })
}

/// JSON helper utilities for safe value traversal and extraction.
pub mod json_utils;
/// Markdown post-processing helpers used by extractors that emit Markdown output.
pub mod markdown_utils;
/// Interned string pool for reducing allocation pressure on repeated strings.
pub mod string_pool;
/// XML helper utilities for tag-name extraction and attribute traversal.
pub mod xml_utils;

/// Helpers for a test that runs one ignored test in a child process with its own environment.
#[cfg(test)]
pub(crate) mod test_support;

#[cfg(feature = "quality")]
pub(crate) use string_utils::safe_decode;
#[cfg(any(feature = "xml", feature = "office"))]
pub(crate) use xml_utils::xml_tag_name;

/// Result of decoding raw bytes into UTF-8, reporting how the text was produced.
///
/// Callers used to infer a lossy decode by checking the returned string for
/// U+FFFD, but that check silently stops working under the `quality` feature: its
/// mojibake cleanup strips every replacement character from the result before a
/// caller can see it. `DecodeOutcome` records provenance at the point where a
/// fallback or replacement actually happens, before any such cleanup runs, so it
/// reports the same thing regardless of which build produced it (#395).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodeOutcome {
    /// The decoded text. Identical to what the corresponding non-provenance decode
    /// helper returns for the same input -- this type only adds observability, it
    /// never changes what gets decoded.
    pub(crate) text: String,
    /// `true` if a non-UTF-8 encoding was used to produce `text` -- whether
    /// explicitly requested, cached from a prior guess, detected, or picked from
    /// the curated single-byte fallback list.
    ///
    /// WHATWG single-byte encodings such as windows-1252 and iso-8859-1 define a
    /// mapping for every byte 0x00-0xFF and can never fail to decode, so this can
    /// be `true` while `replaced_characters` stays `false`: the bytes were
    /// reinterpreted under a different encoding, not lost. Do not treat `fell_back`
    /// alone as evidence of data loss -- check `replaced_characters` for that.
    pub(crate) fell_back: bool,
    /// `true` if the decoder had to substitute one or more U+FFFD replacement
    /// characters because a byte sequence could not be mapped to a character under
    /// the encoding that was actually used. This is genuine, unrecoverable data
    /// loss, unlike `fell_back` on its own.
    pub(crate) replaced_characters: bool,
}

/// Decode raw bytes into UTF-8, reporting fallback/replacement provenance
/// alongside the text (#395).
///
/// This is the unified entry point for both build configurations to get an exact
/// "was this lossy?" answer: under `quality` it delegates to
/// [`string_utils::safe_decode_with_provenance`] (chardetng detection, cached
/// guesses, and the curated single-byte fallback chain), which is also what
/// `safe_decode` itself now routes through. Without `quality` it honors an explicit
/// `encoding` the same way and otherwise falls back to plain
/// `String::from_utf8_lossy`. Both report the same [`DecodeOutcome`] fields for the
/// same input.
#[cfg(feature = "quality")]
pub(crate) fn decode_with_provenance(byte_data: &[u8], encoding: Option<&str>) -> DecodeOutcome {
    // UTF-16 first, as in the build without `quality` below: chardetng does not
    // detect it either (#3539, X54).
    if encoding.is_none()
        && let Some(outcome) = decode_utf16_text(byte_data)
    {
        return outcome;
    }
    string_utils::safe_decode_with_provenance(byte_data, encoding)
}

/// The non-`quality` half of [`decode_with_provenance`]. Mirrors the `quality`
/// branch's explicit-`encoding` handling exactly (both go through `encoding_rs`,
/// which is not itself feature-gated) and otherwise falls back to plain
/// `String::from_utf8_lossy`, detecting replacement the same way
/// `extractors/code.rs` does: `from_utf8_lossy` only allocates (`Cow::Owned`) when
/// it had to substitute a replacement character -- which makes it an exact test for
/// "the input was not valid UTF-8", strictly better than scanning the decoded string
/// for U+FFFD (a document may legitimately contain one).
#[cfg(not(feature = "quality"))]
pub(crate) fn decode_with_provenance(byte_data: &[u8], encoding: Option<&str>) -> DecodeOutcome {
    if byte_data.is_empty() {
        return DecodeOutcome {
            text: String::new(),
            fell_back: false,
            replaced_characters: false,
        };
    }

    // An undeclared UTF-16 text (a BOM, or the zero-byte pattern) is decoded as
    // such; `from_utf8_lossy` below would keep every zero byte (#3539, X54). A
    // declared encoding is honoured as before.
    if encoding.is_none()
        && let Some(outcome) = decode_utf16_text(byte_data)
    {
        return outcome;
    }

    if let Some(enc_name) = encoding
        && let Some(enc) = encoding_rs::Encoding::for_label(enc_name.as_bytes())
    {
        let (decoded, actual_encoding, had_errors) = enc.decode(byte_data);
        return DecodeOutcome {
            text: decoded.into_owned(),
            fell_back: actual_encoding != encoding_rs::UTF_8,
            replaced_characters: had_errors,
        };
    }

    let decoded = String::from_utf8_lossy(byte_data);
    let replaced_characters = matches!(decoded, std::borrow::Cow::Owned(_));
    DecodeOutcome {
        text: decoded.into_owned(),
        fell_back: false,
        replaced_characters,
    }
}

#[cfg(any(
    all(feature = "layout-detection", feature = "pdf"),
    feature = "office",
    feature = "markdown-footnotes"
))]
use std::borrow::Cow;

/// Escape `&`, `<`, and `>` in text destined for markdown/HTML output.
///
/// Underscores are intentionally **not** escaped. In extracted PDF text they are
/// literal content (e.g. identifiers like `CTC_ARP_01`), not markdown italic
/// delimiters.
///
/// Uses a single-pass scan: if no special characters are found, returns a
/// borrowed `Cow` with no allocation.
#[cfg(all(feature = "layout-detection", feature = "pdf"))]
#[inline]
pub(crate) fn escape_html_entities(text: &str) -> Cow<'_, str> {
    let needs_amp = text.contains('&');
    let needs_lt = text.contains('<');
    let needs_gt = text.contains('>');

    if !needs_amp && !needs_lt && !needs_gt {
        return Cow::Borrowed(text);
    }

    let mut result = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(ch),
        }
    }
    Cow::Owned(result)
}

/// Normalizes whitespace by collapsing multiple whitespace characters into single spaces.
/// Returns Cow::Borrowed if no normalization needed.
#[cfg(any(feature = "office", feature = "markdown-footnotes"))]
#[inline]
#[cfg_attr(alef, alef(skip))]
pub(crate) fn normalize_whitespace(s: &str) -> Cow<'_, str> {
    let needs_normalization = s
        .as_bytes()
        .windows(2)
        .any(|w| w[0].is_ascii_whitespace() && w[1].is_ascii_whitespace())
        || s.bytes().any(|b| b != b' ' && b.is_ascii_whitespace())
        || s.as_bytes().first().is_some_and(u8::is_ascii_whitespace)
        || s.as_bytes().last().is_some_and(u8::is_ascii_whitespace);

    if needs_normalization {
        Cow::Owned(s.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(all(test, any(feature = "office", feature = "markdown-footnotes")))]
mod normalize_whitespace_tests {
    use super::*;

    #[test]
    fn borrows_when_already_normalized() {
        let result = normalize_whitespace("already normal");
        assert!(matches!(result, Cow::Borrowed(_)), "should not allocate when unchanged");
        assert_eq!(result, "already normal");
    }

    #[test]
    fn borrows_empty_string() {
        assert!(matches!(normalize_whitespace(""), Cow::Borrowed(_)));
    }

    #[test]
    fn allocates_and_collapses_internal_whitespace_runs() {
        let result = normalize_whitespace("a  b\tc\nd");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "a b c d");
    }

    #[test]
    fn allocates_and_trims_single_leading_and_trailing_space() {
        // A single leading/trailing space has no adjacent whitespace run and no
        // non-space whitespace byte, so it must be checked explicitly: matches
        // `split_whitespace().join(" ")`, which always trims.
        let result = normalize_whitespace(" a b ");
        assert!(
            matches!(result, Cow::Owned(_)),
            "leading/trailing space must not take the borrow fast path"
        );
        assert_eq!(result, "a b");
    }
}

#[cfg(test)]
mod decode_provenance_tests {
    use super::*;

    /// Parity case 1/3: valid UTF-8 must report neither a fallback nor a
    /// replacement, identically in both builds.
    #[test]
    fn valid_utf8_input_reports_no_fallback_and_no_replacement() {
        let input = "Hello, 世界! مرحبا".as_bytes();
        let outcome = decode_with_provenance(input, None);

        assert_eq!(outcome.text, "Hello, 世界! مرحبا");
        assert!(!outcome.fell_back, "valid UTF-8 must not report a fallback");
        assert!(
            !outcome.replaced_characters,
            "valid UTF-8 must not report a replacement"
        );

        #[cfg(feature = "quality")]
        assert_eq!(
            outcome.text,
            safe_decode(input, None),
            "decode_with_provenance must not change the decoded text"
        );
    }

    /// Parity case 2/3: a byte sequence that only decodes under windows-1252 must
    /// report a fallback, but NOT a replacement, in both builds -- the WHATWG
    /// Encoding Standard maps every byte 0x00-0xFF for this encoding, so
    /// reinterpreting under it can never drop data.
    #[test]
    fn windows_1252_reinterpretation_reports_fallback_without_replacement() {
        // 0xE9 is not valid standalone UTF-8, but windows-1252 maps both 0xE9 bytes
        // to 'é' (U+00E9).
        let input: &[u8] = &[b'r', 0xE9, b's', b'u', b'm', 0xE9];
        let outcome = decode_with_provenance(input, Some("windows-1252"));

        assert_eq!(outcome.text, "résumé");
        assert!(
            outcome.fell_back,
            "windows-1252 is not UTF-8, so this must report a fallback"
        );
        assert!(
            !outcome.replaced_characters,
            "windows-1252 maps every byte 0x00-0xFF, so no replacement character can occur"
        );

        #[cfg(feature = "quality")]
        assert_eq!(
            outcome.text,
            safe_decode(input, Some("windows-1252")),
            "decode_with_provenance must not change the decoded text"
        );
    }

    /// Parity case 3/3: bytes that are genuinely undecodable as UTF-8 must report a
    /// replacement in BOTH builds -- this is the case that was undetectable before
    /// #395, because `quality`'s mojibake cleanup used to strip the only evidence.
    /// The text itself still legitimately differs between builds (that cleanup is
    /// unchanged, pre-existing behavior): `quality` strips the U+FFFD characters,
    /// non-`quality` leaves them in place. Only the new `replaced_characters` flag
    /// is required to agree.
    #[test]
    fn forced_replacement_reports_replacement_in_both_builds() {
        let input: &[u8] = &[b'A', 0xFF, 0xFE, b'B'];
        let outcome = decode_with_provenance(input, Some("utf-8"));

        #[cfg(feature = "quality")]
        assert_eq!(
            outcome.text, "AB",
            "quality's mojibake cleanup strips U+FFFD -- pre-existing behavior"
        );
        #[cfg(not(feature = "quality"))]
        assert_eq!(
            outcome.text, "A\u{FFFD}\u{FFFD}B",
            "non-quality leaves U+FFFD in place -- pre-existing behavior"
        );

        assert!(!outcome.fell_back, "UTF-8 was used, so this must not report a fallback");
        assert!(
            outcome.replaced_characters,
            "undecodable bytes must be reported as a replacement in both builds"
        );

        #[cfg(feature = "quality")]
        assert_eq!(
            outcome.text,
            safe_decode(input, Some("utf-8")),
            "decode_with_provenance must not change the decoded text"
        );
    }

    #[test]
    fn empty_input_reports_no_fallback_and_no_replacement() {
        let outcome = decode_with_provenance(b"", None);

        assert_eq!(outcome.text, "");
        assert!(!outcome.fell_back);
        assert!(!outcome.replaced_characters);
    }
}

/// #3539 (X54): UTF-16 text is recognised by its BOM or its zero-byte pattern.
#[cfg(test)]
mod utf16_text_tests {
    use super::*;

    const JUMO: &str = "\"Analog\",\"\"\r\n\"Archiv: JUMO 1, Run 1 - JUMO LOGOSCREEN 500 cf\"\r\n\"Datum\",\"Zeit\",\"LOW: T1 / °C\"\r\n\"1\",\"18-3-2011\",\"9:56:31\",\"8,85\"\r\n";

    fn utf16le(text: &str, bom: bool) -> Vec<u8> {
        let mut out = if bom { vec![0xFF, 0xFE] } else { vec![] };
        out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        out
    }

    fn utf16be(text: &str, bom: bool) -> Vec<u8> {
        let mut out = if bom { vec![0xFE, 0xFF] } else { vec![] };
        out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        out
    }

    #[test]
    fn a_bom_or_the_zero_byte_pattern_names_utf16() {
        assert_eq!(
            utf16_text_encoding(&utf16le(JUMO, true)),
            Some((encoding_rs::UTF_16LE, 2))
        );
        assert_eq!(
            utf16_text_encoding(&utf16be(JUMO, true)),
            Some((encoding_rs::UTF_16BE, 2))
        );
        assert_eq!(
            utf16_text_encoding(&utf16le(JUMO, false)),
            Some((encoding_rs::UTF_16LE, 0))
        );
        assert_eq!(
            utf16_text_encoding(&utf16be(JUMO, false)),
            Some((encoding_rs::UTF_16BE, 0))
        );
    }

    #[test]
    fn eight_bit_text_and_the_rest_are_not_utf16() {
        // UTF-8, with and without its BOM, and windows-1252: no zero bytes.
        assert_eq!(utf16_text_encoding(JUMO.as_bytes()), None);
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend(JUMO.as_bytes());
        assert_eq!(utf16_text_encoding(&utf8_bom), None);
        let (cp1252, _, _) = encoding_rs::WINDOWS_1252.encode(JUMO);
        assert_eq!(utf16_text_encoding(&cp1252), None);
        // The UTF-32LE BOM is not taken for UTF-16LE.
        assert_eq!(utf16_text_encoding(&[0xFF, 0xFE, 0x00, 0x00, b'A', 0, 0, 0]), None);
        // Too short to tell without a BOM, and mostly non-Latin UTF-16 without one.
        assert_eq!(utf16_text_encoding(&utf16le("Ab", false)), None);
        assert_eq!(utf16_text_encoding(&utf16le("温度測定の記録データです", false)), None);
        // A binary-looking run of zeros is not UTF-16 either.
        assert_eq!(utf16_text_encoding(&[0u8; 64]), None);
    }

    #[test]
    fn utf16_text_decodes_to_its_utf8_twin() {
        for bytes in [
            utf16le(JUMO, true),
            utf16be(JUMO, true),
            utf16le(JUMO, false),
            utf16be(JUMO, false),
        ] {
            let outcome = decode_with_provenance(&bytes, None);
            assert_eq!(outcome.text, JUMO);
            assert!(outcome.fell_back);
            assert!(!outcome.replaced_characters);
        }
    }

    /// The zero-byte sniff only runs without a declared encoding. (A BOM is honoured
    /// over a declared label by `encoding_rs` itself, as before.)
    #[test]
    fn a_declared_encoding_is_not_second_guessed() {
        let bytes = utf16le(JUMO, false);
        let outcome = decode_with_provenance(&bytes, Some("windows-1252"));
        assert_ne!(outcome.text, JUMO);
        assert!(outcome.text.contains('\0'));
    }
}
