//! Shared helpers for the integration suite.
//!
//! Integration tests are separate crates, so this is included with `mod common;`
//! rather than being a library. Each test binary uses only part of it, hence the
//! crate-level `dead_code` allow.
//!
//! `fixture-hygiene` requires reproducers to be minimal PDFs constructed in
//! code. Before this module the same builder was copy-pasted into a dozen test
//! files — eleven of them byte-identical — which is how they drifted apart.

#![allow(dead_code)]

/// Write `data` to a uniquely-named file inside a fresh temporary directory
/// and return both. Each call gets its own directory (via `tempfile`), so
/// concurrent test processes never observe one another's writes — unlike a
/// fixed path under `std::env::temp_dir()`, which is shared by every process
/// on the machine.
///
/// Keep the returned `TempDir` alive for as long as the path is used: it
/// deletes the directory (and the file in it) when dropped. Use the same
/// `TempDir` (via `.path()`) to build sibling paths, e.g. an output file
/// written alongside the input.
pub fn write_temp_pdf(data: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, data).expect("Failed to write temp file");
    (dir, path)
}

pub fn build_minimal_pdf_raw(content: &[u8], page_extra: &[u8]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(b"3 0 obj\n<< ");
    pdf.extend_from_slice(page_extra);
    pdf.extend_from_slice(b" /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes());
    pdf.extend_from_slice(content);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    offsets.push(pdf.len());
    pdf.extend_from_slice(
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
          /Encoding /WinAnsiEncoding >>\nendobj\n",
    );

    finalize_pdf(pdf, &offsets)
}

/// Append the cross-reference table, trailer and `startxref` for a body whose
/// object offsets are `offsets` (index 0 is the free head and is ignored).
pub fn finalize_pdf(mut pdf: Vec<u8>, offsets: &[usize]) -> Vec<u8> {
    let xref_pos = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f\r\n");
    for &off in &offsets[1..] {
        pdf.extend_from_slice(format!("{off:010} 00000 n\r\n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_pos
        )
        .as_bytes(),
    );
    pdf
}
