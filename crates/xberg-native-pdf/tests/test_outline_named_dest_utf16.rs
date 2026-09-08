//! Reproduction for named destinations stored as UTF-16BE strings — the form
//! Adobe Distiller emits, and the one journal PDFs from Springer Nature and
//! Frontiers carry.
//!
//! `/Dest (name)` is a **name-tree key**, not a text string: ISO 32000-1 §7.9.6
//! orders and compares those keys lexically by byte. Decoding the name before
//! the lookup therefore breaks it — the `FE FF` BOM is not valid UTF-8, so a
//! round-trip through `String::from_utf8_lossy` replaces it with two U+FFFD and
//! the comparison against the raw tree key can never match.
//!
//! The sibling `test_outline_utf16.rs` covers the *title* half of the same
//! encoding: those decode, because a `/Title` really is a text string.

use xberg_native_pdf::document::PdfDocument;
use xberg_native_pdf::outline::Destination;

/// UTF-16BE, BOM first — what `/Dest` carries in the real files.
fn utf16be(s: &str) -> Vec<u8> {
    let mut out = vec![0xFEu8, 0xFF];
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

/// A two-page PDF whose second page is the target of three destinations:
/// a direct array, a named one keyed by `key`, and an ASCII-named control in
/// the same tree. `key` is the only thing that varies between the failing and
/// the passing shape.
fn build_pdf(key: &[u8]) -> Vec<u8> {
    let key_hex = hex(key);
    let objects: Vec<Vec<u8>> = vec![
        // 1: Catalog
        b"<< /Type /Catalog /Pages 2 0 R /Names 5 0 R /Outlines 7 0 R >>".to_vec(),
        // 2: Pages
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
        // 3: Page one
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_vec(),
        // 4: Page two — every destination below targets it
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_vec(),
        // 5: /Names
        b"<< /Dests 6 0 R >>".to_vec(),
        // 6: the /Dests name tree — one leaf, two keys. `key` sorts before
        //    (ZZ) in both shapes, so the array is in the byte order §7.9.6
        //    requires.
        format!("<< /Names [ <{key_hex}> << /D [4 0 R /Fit] >> (ZZ) << /D [4 0 R /Fit] >> ] >>").into_bytes(),
        // 7: Outlines root
        b"<< /Type /Outlines /First 8 0 R /Last 10 0 R /Count 3 >>".to_vec(),
        // 8: control — a direct destination array, which never needed a lookup
        b"<< /Title (Direct) /Parent 7 0 R /Next 9 0 R /Dest [4 0 R /Fit] >>".to_vec(),
        // 9: the case under test
        format!("<< /Title (Named) /Parent 7 0 R /Prev 8 0 R /Next 10 0 R /Dest <{key_hex}> >>").into_bytes(),
        // 10: control — an ASCII name in the same tree, so a failure here
        //     would mean the tree walk itself is broken rather than the key
        b"<< /Title (Named ASCII) /Parent 7 0 R /Prev 9 0 R /Dest (ZZ) >>".to_vec(),
    ];

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = pdf.len();
    let n = objects.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {}\n", n).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            n, xref_offset
        )
        .as_bytes(),
    );
    pdf
}

fn page_indices(key: &[u8]) -> Vec<Option<usize>> {
    let doc = PdfDocument::from_bytes(build_pdf(key)).expect("open");
    let outline = doc.get_outline().expect("get_outline").expect("some");
    outline
        .iter()
        .map(|item| match item.dest {
            Some(Destination::PageIndex(i)) => Some(i),
            _ => None,
        })
        .collect()
}

#[test]
fn utf16be_named_destination_resolves() {
    // Before the fix the middle item came back as
    // `Destination::Named("\u{fffd}\u{fffd}\0A\03")` with no page, while its
    // two neighbours resolved — the tree was reachable, the key was not.
    assert_eq!(page_indices(&utf16be("A3")), vec![Some(1), Some(1), Some(1)]);
}

#[test]
fn ascii_named_destination_still_resolves() {
    // The same document with an ASCII key, which worked before the fix too:
    // the byte-taking lookup must not regress it.
    assert_eq!(page_indices(b"A3"), vec![Some(1), Some(1), Some(1)]);
}

#[test]
fn resolve_named_destination_bytes_takes_the_raw_key() {
    let key = utf16be("A3");
    let doc = PdfDocument::from_bytes(build_pdf(&key)).expect("open");

    assert_eq!(
        doc.resolve_named_destination_bytes(&key).expect("resolve"),
        Some(1),
        "the raw key resolves"
    );
    // The decoded text is not the key — the tree really is keyed by the
    // UTF-16BE bytes, so declining this is correct, not a second bug.
    assert_eq!(
        doc.resolve_named_destination("A3").expect("resolve"),
        None,
        "the decoded text is a different byte string"
    );
    // The ASCII sibling still resolves through the `&str` entry point.
    assert_eq!(doc.resolve_named_destination("ZZ").expect("resolve"), Some(1));
}
