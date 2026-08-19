//! Cache-key version tag folded into every on-disk cache key.
//!
//! A cached extraction result is only valid for the crate version and cache
//! schema that produced it. Neither the content hash (bytes of the input) nor
//! the config hash (the `ExtractionConfig`) changes when *extraction
//! behaviour* changes — a fixed extractor, a reordered pipeline stage, a new
//! post-processor — so without this tag an entry written before the fix is
//! served forever, and the bug it encodes outlives its own fix.
//!
//! This is NOT a build fingerprint: it is derived only from `CARGO_PKG_VERSION`
//! and [`CACHE_SCHEMA_VERSION`], with no git SHA, build id, or compile
//! timestamp. Two separately built binaries that share a crate version and
//! schema version produce the identical tag and therefore share cache
//! entries, even when the code between them differs. A behaviour change that
//! isn't paired with a crate version bump MUST bump [`CACHE_SCHEMA_VERSION`]
//! to invalidate the entries it would otherwise silently share.
//!
//! Every cache key is therefore prefixed with this tag. Changing either the
//! crate version or [`CACHE_SCHEMA_VERSION`] makes all previously written
//! entries unreachable; they age out through the normal cleanup pass.

/// Generation counter for cached extraction results.
///
/// Bump this whenever extraction output can change for an unchanged input and
/// an unchanged `ExtractionConfig` — that is, whenever a fix or a behaviour
/// change would otherwise be masked by an entry written before it landed.
/// Bumping it invalidates every existing cache entry process-wide.
/// Bumped for #687: the OCR cache key did not cover the Tesseract engine variables
/// `apply_tesseract_variables` applies (`crates/xberg/src/ocr/processor/config.rs`), so
/// entries written before `hocr_font_info` was enabled in 57e414a6db kept being served
/// after it landed, serving stale font-size data. `hash_config` now folds those variables
/// into its own hash, but this bump is still needed to invalidate the entries that were
/// already on disk before that fix.
pub(crate) const CACHE_SCHEMA_VERSION: u32 = 3;

/// Number of hex characters in the cache version tag.
const VERSION_TAG_HEX_LEN: usize = 8;

/// Return the process-wide cache-key version tag as 8 hex characters.
///
/// Stable for the lifetime of a process: the same binary always produces the
/// same tag. Two builds that differ in crate version or [`CACHE_SCHEMA_VERSION`]
/// always produce different tags — but this is NOT a build fingerprint. It
/// distinguishes crate-version/schema-version pairs only; it does not
/// distinguish two separately built binaries that share both (no git SHA, no
/// build id, no compile timestamp folded in), so such builds share cache
/// entries.
pub(crate) fn cache_version_tag() -> &'static str {
    static TAG: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    TAG.get_or_init(|| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
        hasher.update(b"\x00");
        hasher.update(&CACHE_SCHEMA_VERSION.to_le_bytes());
        hex::encode(&hasher.finalize().as_bytes()[..VERSION_TAG_HEX_LEN / 2])
    })
    .as_str()
}

/// Prefix `cache_key` with the cache-key version tag (see [`cache_version_tag`]).
///
/// The result stays a single safe filename component: the tag is hex, and the
/// separator is `-`, so `Path::file_stem` on `<tag>-<key>.msgpack` round-trips
/// back to the versioned key.
pub(crate) fn versioned_cache_key(cache_key: &str) -> String {
    format!("{}-{}", cache_version_tag(), cache_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_is_eight_lowercase_hex_characters() {
        let tag = cache_version_tag();
        assert_eq!(tag.len(), VERSION_TAG_HEX_LEN, "tag was {tag:?}");
        assert!(
            tag.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "tag was {tag:?}"
        );
    }

    #[test]
    fn tag_is_stable_across_calls() {
        assert_eq!(cache_version_tag(), cache_version_tag());
    }

    #[test]
    fn versioned_key_prefixes_the_tag_and_preserves_the_key() {
        let versioned = versioned_cache_key("deadbeefdeadbeefdeadbeefdeadbeef");
        assert_eq!(
            versioned,
            format!("{}-deadbeefdeadbeefdeadbeefdeadbeef", cache_version_tag())
        );
        assert!(versioned.ends_with("-deadbeefdeadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn versioned_key_is_deterministic_for_equal_keys_and_distinct_for_different_keys() {
        assert_eq!(versioned_cache_key("aaaa"), versioned_cache_key("aaaa"));
        assert_ne!(versioned_cache_key("aaaa"), versioned_cache_key("bbbb"));
    }

    #[test]
    fn versioned_key_stays_a_single_path_component() {
        let versioned = versioned_cache_key("deadbeef");
        assert!(!versioned.contains('/'), "{versioned}");
        assert!(!versioned.contains('\\'), "{versioned}");
        assert_eq!(
            std::path::Path::new(&format!("{versioned}.msgpack"))
                .file_stem()
                .and_then(|s| s.to_str()),
            Some(versioned.as_str()),
            "file_stem must round-trip back to the versioned key"
        );
    }

    /// Guards the whole point of #206: a different schema generation must not be
    /// able to collide with the current one. Recomputes the tag the way
    /// [`cache_version_tag`] does, but for a bumped schema version.
    #[test]
    fn bumping_the_schema_version_changes_the_tag() {
        fn tag_for(crate_version: &str, schema_version: u32) -> String {
            let mut hasher = blake3::Hasher::new();
            hasher.update(crate_version.as_bytes());
            hasher.update(b"\x00");
            hasher.update(&schema_version.to_le_bytes());
            hex::encode(&hasher.finalize().as_bytes()[..VERSION_TAG_HEX_LEN / 2])
        }

        let current = tag_for(env!("CARGO_PKG_VERSION"), CACHE_SCHEMA_VERSION);
        let bumped = tag_for(env!("CARGO_PKG_VERSION"), CACHE_SCHEMA_VERSION + 1);
        let other_crate_version = tag_for("0.0.0-test", CACHE_SCHEMA_VERSION);

        assert_eq!(current, cache_version_tag(), "helper must match the real derivation");
        assert_ne!(current, bumped, "a schema bump must invalidate old entries");
        assert_ne!(
            current, other_crate_version,
            "a version bump must invalidate old entries"
        );
    }
}
