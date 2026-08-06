//! Regression test for GH#1387 / internal issue #308.
//!
//! `list_supported_formats()` used to map the static `EXT_TO_MIME` table straight into
//! its result with no feature filtering, so a narrower build (fewer Cargo features, or a
//! narrower `xberg-ffi` target dependency list) would advertise formats whose extractors
//! were compiled out. Every FFI binding's `ListSupportedFormats()` inherited the lie.
//!
//! The fix intersects the static table with the document extractor registry's live MIME
//! index, so the advertised catalogue can never claim a format this build cannot actually
//! extract. This test is table-driven over every entry `list_supported_formats()` returns
//! and asserts each one resolves to a registered extractor — it is meant to be run under
//! more than one feature set so the defect class (advertise-without-register) is
//! self-detecting rather than tied to one particular build's format list.
//!
//! This test only *reads* the process-global extractor registry (via the public
//! `ensure_initialized` + `list_supported_formats` APIs) — it does not register, clear, or
//! otherwise mutate it, so it is safe to run concurrently with other tests that exercise
//! registry lifecycle/isolation behavior.

use xberg::core::mime::list_supported_formats;
use xberg::extractors::ensure_initialized;
use xberg::plugins::registry::get_document_extractor_registry;

/// Every extension `list_supported_formats()` advertises must resolve to a MIME type that
/// some extractor is actually registered for in this build.
#[test]
fn every_advertised_format_resolves_to_a_registered_extractor() {
    ensure_initialized().expect("built-in extractor registration should succeed");

    let formats = list_supported_formats();
    assert!(!formats.is_empty(), "list_supported_formats() should not be empty");

    let registry = get_document_extractor_registry();
    let registry_guard = registry.read();

    let unregistered: Vec<String> = formats
        .iter()
        .filter(|format| registry_guard.get(&format.mime_type).is_err())
        .map(|format| format!("{} ({})", format.extension, format.mime_type))
        .collect();

    assert!(
        unregistered.is_empty(),
        "list_supported_formats() advertised formats with no registered extractor in this \
         build (GH#1387 regression): {unregistered:?}"
    );
}

/// The list must stay sorted by extension regardless of how many entries the registry
/// intersection filters out.
#[test]
fn list_stays_sorted_after_registry_filtering() {
    ensure_initialized().expect("built-in extractor registration should succeed");

    let formats = list_supported_formats();
    let extensions: Vec<&str> = formats.iter().map(|format| format.extension.as_str()).collect();
    let mut sorted = extensions.clone();
    sorted.sort_unstable();

    assert_eq!(extensions, sorted, "formats must remain sorted by extension after filtering");
}

/// Formats that are always registered regardless of optional Cargo features (the
/// unconditional built-ins: plain text, markdown, structured/JSON, CSV, HTML) must survive
/// the registry intersection in every build, including the narrowest ones.
#[test]
fn unconditionally_registered_formats_always_survive_filtering() {
    ensure_initialized().expect("built-in extractor registration should succeed");

    let formats = list_supported_formats();
    let extensions: std::collections::HashSet<&str> =
        formats.iter().map(|format| format.extension.as_str()).collect();

    for ext in ["txt", "md", "csv", "json", "html"] {
        assert!(
            extensions.contains(ext),
            "extension `{ext}` is served by an unconditionally-registered extractor and \
             should always be present regardless of optional feature flags"
        );
    }
}
