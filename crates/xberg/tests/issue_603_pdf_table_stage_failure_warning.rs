//! Regression tests for commit 5d6e37e863 ("fix(pdf): report table failures that
//! take out a whole detector pass"), `crates/xberg/src/extractors/pdf/extraction.rs`.
//!
//! That commit added `table_stage_failure_warning` and wired it into the four
//! `unwrap_or_else`/`Err` fallbacks that substitute an empty table result when a
//! whole table-detection pass fails outright, rather than just one page:
//!
//! * the native pass's `unwrap_or_else` (extraction.rs, `extract_tables_native` call)
//! * the bordered pass's `unwrap_or_else` (extraction.rs, `extract_tables_bordered` call)
//! * the heuristic pass's `Err` arm (extraction.rs, `extract_tables_heuristic` call)
//! * the `guard_oxide_panic` fallback around the whole table-detection closure
//!   (extraction.rs, "whole-document" stage)
//!
//! Before that commit, all four substituted `Vec::new()` for the warnings, so a
//! pass that failed or panicked came back indistinguishable from a PDF that
//! genuinely has no tables — this is exactly the gap the finding against #603
//! describes as having zero test coverage.
//!
//! Verification that the gap was real: grepping the test suite (`crates/xberg/tests/`
//! and `crates/xberg/src/**`) for `pdf_tables` (the warning's `source`),
//! `table_stage_failure_warning`, and each of the four literal warning-message
//! fragments this commit introduced ("native table extraction failed for the
//! document", "bordered table extraction failed for the document", "heuristic
//! table extraction failed for the document", "whole-document table extraction
//! failed for the document") turned up no hits outside
//! `crates/xberg/src/extractors/pdf/extraction.rs` and
//! `crates/xberg/src/pdf/oxide/table.rs` themselves — nothing anywhere asserted on
//! these warnings.
//!
//! Only the "whole-document" (panic-guard) path is exercised here with a real
//! failure. The native and bordered passes only return `Err` when
//! `OxideDocument::doc.page_count()` fails (`pdf/oxide/table.rs`), but
//! `extract_text_and_metadata` — which runs earlier in the same pipeline and
//! whose own `?` would abort the whole extraction first — already calls
//! `page_count()` successfully on the same document handle
//! (`pdf/oxide/text.rs`), so that Err arm cannot be reached through the public
//! API without corrupting the document's internal state between calls, which is
//! not something a test can do without either a production-code seam or a
//! pdf_oxide-internal PDF corpus this repository does not have. The same applies
//! to the heuristic pass's hierarchy-extraction `Err` arm
//! (`pdf/oxide/hierarchy.rs`) for its `page_count()` fallback. These are noted
//! as untestable through the public API rather than left silently uncovered.

#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)] // ~keep: test/bench binaries print by design; org logging policy exempts tests
#![cfg(feature = "pdf")]

mod helpers;
use helpers::{extract_bytes_document_blocking, extract_uri_document_blocking, get_test_file_path};

use xberg::ExtractionConfig;

/// Read a repro PDF from `test_documents/pdf/`, returning `None` when the
/// submodule is not present so the test skips rather than fails (same
/// skip-on-missing-fixture convention as `pdf_oxide_total_order_panic_1198.rs`).
fn read_repro(name: &str) -> Option<Vec<u8>> {
    let path = get_test_file_path(&format!("pdf/{name}"));
    if !path.exists() {
        eprintln!(
            "skipping: repro PDF not found at {} (test_documents submodule?)",
            path.display()
        );
        return None;
    }
    Some(std::fs::read(&path).expect("read repro PDF"))
}

/// Depends on: the `guard_oxide_panic` fallback arm added in extraction.rs —
/// `.unwrap_or_else(|e| { ...; vec![table_stage_failure_warning("whole-document", &e)] })`
/// — which is the only one of the commit's four fallbacks reachable through the
/// public API with a real (not constructed) failure. `total_order_panic_1198_tables_path.pdf`
/// is the exact fixture already used by `pdf_oxide_total_order_panic_1198.rs` to
/// prove the table-detection closure genuinely panics on this file (tategaki
/// total-order violation), so `guard_oxide_panic` genuinely catches a real panic
/// here rather than the test fabricating one.
///
/// Reverting the commit removes `table_stage_failure_warning` and its four call
/// sites entirely (a compile error), so this test cannot even build against a
/// reverted tree; reverting only the "whole-document" call site back to
/// `(Vec::new(), None, Vec::new())` leaves `processing_warnings` without the
/// `pdf_tables`-sourced entry this test asserts on, failing it.
#[test]
fn should_emit_pdf_tables_warning_when_whole_document_table_pass_panics() {
    let Some(bytes) = read_repro("total_order_panic_1198_tables_path.pdf") else {
        return;
    };
    let config = ExtractionConfig::default();

    let result = extract_bytes_document_blocking(&bytes, "application/pdf", &config)
        .expect("a table-stage panic must fall back to no tables, not abort the whole extraction");

    let table_warnings: Vec<_> = result
        .processing_warnings
        .iter()
        .filter(|warning| warning.source == "pdf_tables")
        .collect();

    assert_eq!(
        table_warnings.len(),
        1,
        "exactly one pdf_tables stage-failure warning is expected for a single panicking pass, got: {:?}",
        result.processing_warnings
    );

    let warning = table_warnings[0];
    assert_eq!(
        warning.source, "pdf_tables",
        "table_stage_failure_warning hardcodes source = Cow::Borrowed(\"pdf_tables\")"
    );

    let message = warning.message.as_ref();
    let expected_prefix = "whole-document table extraction failed for the document: ";
    let expected_suffix = "; tables from this pass were skipped";
    assert!(
        message.starts_with(expected_prefix),
        "message must name the whole-document stage per table_stage_failure_warning's format string, got: {message}"
    );
    assert!(
        message.ends_with(expected_suffix),
        "message must name that tables from this pass were skipped, got: {message}"
    );
    assert!(
        message.contains("pdf_oxide panicked during table extraction"),
        "message must carry the panic-guard's cause text (extraction.rs's `on_panic` closure), got: {message}"
    );

    // Partial-results contract: a stage-level table failure must not discard the
    // rest of the document. `extract_all_from_oxide_document` still returns the
    // page text produced earlier in the same function, independent of the table
    // phase's outcome.
    assert!(
        !result.content.trim().is_empty(),
        "the panicking table pass must not discard the document's extracted text"
    );
    assert!(
        result.tables.is_empty(),
        "the panicking pass falls back to an empty table list, not partial/garbage tables"
    );
}

/// Depends on: proving the four `table_stage_failure_warning` call sites are
/// conditional on their respective pass actually failing — none of them fire
/// unconditionally. A warning source that always appears would be exactly as
/// broken as one that never does; only a genuine, unmodified successful
/// extraction distinguishes the two, so this deliberately does NOT construct a
/// warning or stub any pass — it runs a real, valid PDF through the same
/// `extract_bytes_document_blocking` entry point as the panic test above.
#[test]
fn should_not_emit_pdf_tables_warning_on_a_successful_extraction() {
    let path = get_test_file_path("pdf/simple.pdf");
    if !path.exists() {
        eprintln!(
            "skipping: fixture not found at {} (test_documents submodule?)",
            path.display()
        );
        return;
    }
    let config = ExtractionConfig::default();

    let result =
        extract_uri_document_blocking(&path, None, &config).expect("a valid, non-panicking PDF must extract cleanly");

    let table_warnings: Vec<_> = result
        .processing_warnings
        .iter()
        .filter(|warning| warning.source == "pdf_tables")
        .collect();

    assert!(
        table_warnings.is_empty(),
        "no table-detection pass failed for this document, so no pdf_tables stage-failure warning \
         should be present, got: {table_warnings:?}"
    );
}
