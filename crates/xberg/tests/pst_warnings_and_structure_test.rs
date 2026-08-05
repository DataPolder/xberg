//! Integration coverage for PST warning propagation and folder-structure
//! output, related to issues #151 and #162.
//!
//! # Fixture limitation
//!
//! The only PST fixture checked into the repository is `test_documents/email/empty.pst`,
//! which is a well-formed, empty PST: it has a locatable IPM (mail) sub-tree, no
//! non-IPM top-level folders, and no folder nesting deep enough to hit the
//! depth-50 traversal cap. That means the specific warning-emission branches for
//! issue #162 -- (a) missing/unopenable IPM sub-tree and (b) depth-cap truncation --
//! cannot be exercised end-to-end with this fixture. (a) is covered by a unit test in
//! `crates/xberg/src/extraction/pst.rs` (`test_extract_from_store_warns_when_ipm_subtree_missing_issue_162`),
//! which exercises the traversal logic directly against a synthetic `Store`
//! implementation instead of a real PST file. (b) is implemented (see
//! `extract_from_store`'s `depth > 50` branch) but not covered by an automated
//! test: driving the traversal loop deep enough to hit the cap requires a
//! synthetic `Store` whose `StoreProperties::make_entry_id` succeeds, which in
//! turn requires a record key (property `0xFF9`) that `StoreProperties` has no
//! public constructor/setter for (only `Default`, which yields an empty
//! property map) -- so it was verified by manual code review only.
//!
//! Issue #162(c) -- warning about non-IPM top-level folders skipped by
//! extraction -- was attempted (enumerating `Store::root_hierarchy_table()`)
//! but reverted: calling it against a real, parsed PST file (`empty.pst`)
//! caused `test_pst_empty_file_extraction` to hang indefinitely. This appears
//! to be a latent issue in how the `outlook-pst` crate resolves the root
//! hierarchy table (a code path not otherwise exercised by this codebase, since
//! only the IPM sub-tree was ever walked before), not a fixable bug in the two
//! files owned by this change. Part (c) was therefore left unimplemented; see
//! the accompanying report for the recommended follow-up.
//!
//! What *is* verified here, end-to-end through the public extraction API:
//! - A well-formed PST with no warning conditions produces an empty
//!   `processing_warnings` list (no false positives introduced by the #151/#162
//!   fixes).
//! - `processing_warnings` on the public `ExtractedDocument` is reachable and
//!   correctly typed, confirming the plumbing wired up in `extractors/pst.rs`
//!   compiles and runs against a real PST file.

#![cfg(feature = "email")]

use xberg::core::config::ExtractionConfig;

mod helpers;
use helpers::extract_bytes_document;

/// A well-formed, empty PST should not produce any spurious processing
/// warnings now that #151 propagates warnings and #162 adds new
/// warning-emission paths -- neither should fire for a clean file.
#[tokio::test]
async fn test_pst_empty_file_has_no_spurious_processing_warnings() {
    let pst_path = helpers::get_test_file_path("email/empty.pst");
    assert!(pst_path.exists(), "PST test fixture not found: {:?}", pst_path);

    let pst_bytes = std::fs::read(&pst_path).expect("Should read empty.pst");
    let config = ExtractionConfig::default();

    let result = extract_bytes_document(&pst_bytes, "application/vnd.ms-outlook-pst", &config)
        .await
        .expect("Should extract empty PST without error");

    assert!(
        result.processing_warnings.is_empty(),
        "well-formed empty PST should not produce warnings, got: {:?}",
        result.processing_warnings
    );
}
