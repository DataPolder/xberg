//! Regression tests for issue #337.
//!
//! `ChunkingConfig::breadcrumb_target` controls *where* the Markdown heading-path
//! breadcrumb is written when `prepend_heading_context` is enabled: into chunk
//! `content` (good for dense/embedding retrieval), into `ChunkMetadata::heading_path`
//! only (good for lexical retrieval, since every chunk under a heading otherwise
//! repeats the same tokens and collapses that heading word's IDF toward zero), or
//! both. Before this knob existed, a caller who wanted clean `content` for a BM25
//! index had to fragile-string-strip the breadcrumb prefix back off using
//! `heading_path` — this suite locks down all three variants plus the default.
#![cfg(feature = "chunking")]

use xberg::chunking::{ChunkerType, ChunkingConfig, chunk_for_rag};
use xberg::BreadcrumbTarget;

/// A single H1 heading (`# Setup`) at byte offset 0, followed by prose long enough
/// (with `max_characters` below) to force multiple chunks. The heading text is the
/// document's only `#` character, so any chunk whose `byte_start` is greater than 0
/// is *guaranteed* not to start with a `#` — its raw content can never be mistaken
/// for a heading line by `strip_leading_heading`, making the breadcrumb-prepend
/// transformation on those chunks fully predictable without depending on exactly
/// where the underlying Markdown splitter chose to cut.
const MARKDOWN: &str = concat!(
    "# Setup\n\n",
    "First paragraph has enough distinct words to occupy a good portion of the very first chunk by itself here.\n\n",
    "Second paragraph continues on a totally different note with entirely separate content and phrasing here.\n\n",
    "Third paragraph wraps up the section with a final thought that is unrelated to the heading text above.",
);

/// `max_characters` deliberately smaller than `MARKDOWN.len()`: a chunker's content
/// contract guarantees no single chunk exceeds this cap, so an input longer than the
/// cap is guaranteed to produce more than one chunk — regardless of the splitter's
/// internal boundary choices.
const MAX_CHARACTERS: usize = 60;

fn config(breadcrumb_target: BreadcrumbTarget) -> ChunkingConfig {
    ChunkingConfig {
        max_characters: MAX_CHARACTERS,
        overlap: 0,
        trim: true,
        chunker_type: ChunkerType::Markdown,
        prepend_heading_context: true,
        breadcrumb_target,
        ..Default::default()
    }
}

#[test]
fn default_breadcrumb_target_pins_todays_content_prepending_behaviour() {
    assert_eq!(
        ChunkingConfig::default().breadcrumb_target,
        BreadcrumbTarget::Content,
        "the default must preserve the content-prepend behaviour that existed before this option (#337)"
    );
}

#[test]
fn default_config_matches_explicit_content_target() {
    let default_config = ChunkingConfig {
        max_characters: MAX_CHARACTERS,
        overlap: 0,
        trim: true,
        chunker_type: ChunkerType::Markdown,
        prepend_heading_context: true,
        // `breadcrumb_target` intentionally left unset — exercises the default.
        ..Default::default()
    };
    let explicit_content_config = config(BreadcrumbTarget::Content);

    let default_result = chunk_for_rag(MARKDOWN, &default_config).unwrap();
    let explicit_result = chunk_for_rag(MARKDOWN, &explicit_content_config).unwrap();

    assert_eq!(default_result.chunks.len(), explicit_result.chunks.len());
    for (default_chunk, explicit_chunk) in default_result.chunks.iter().zip(explicit_result.chunks.iter()) {
        assert_eq!(
            default_chunk.content, explicit_chunk.content,
            "an unset breadcrumb_target must produce byte-identical content to an explicit Content target"
        );
    }
}

#[test]
fn metadata_target_never_mutates_content() {
    let metadata_result = chunk_for_rag(MARKDOWN, &config(BreadcrumbTarget::Metadata)).unwrap();

    let mut prepend_disabled_config = config(BreadcrumbTarget::Content);
    prepend_disabled_config.prepend_heading_context = false;
    let prepend_disabled_result = chunk_for_rag(MARKDOWN, &prepend_disabled_config).unwrap();

    assert_eq!(metadata_result.chunks.len(), prepend_disabled_result.chunks.len());
    for (metadata_chunk, disabled_chunk) in metadata_result.chunks.iter().zip(prepend_disabled_result.chunks.iter()) {
        assert_eq!(
            metadata_chunk.content, disabled_chunk.content,
            "Metadata target must leave content exactly as if prepend_heading_context were disabled"
        );
    }
}

#[test]
fn metadata_target_still_populates_heading_path() {
    let result = chunk_for_rag(MARKDOWN, &config(BreadcrumbTarget::Metadata)).unwrap();
    assert!(!result.chunks.is_empty());
    for chunk in &result.chunks {
        assert_eq!(
            chunk.metadata.heading_path,
            vec!["Setup".to_string()],
            "heading_path must be populated regardless of breadcrumb_target, content: {:?}",
            chunk.content
        );
        assert!(
            !chunk.content.contains(" > "),
            "Metadata target must never write the breadcrumb separator into content, got: {:?}",
            chunk.content
        );
    }
}

#[test]
fn content_target_prepends_breadcrumb_for_chunks_starting_after_the_heading_line() {
    let metadata_result = chunk_for_rag(MARKDOWN, &config(BreadcrumbTarget::Metadata)).unwrap();
    let content_result = chunk_for_rag(MARKDOWN, &config(BreadcrumbTarget::Content)).unwrap();

    assert_eq!(metadata_result.chunks.len(), content_result.chunks.len());
    assert!(
        metadata_result.chunks.len() >= 2,
        "MAX_CHARACTERS must be small enough relative to MARKDOWN to force multiple chunks; got {} chunk(s)",
        metadata_result.chunks.len()
    );

    let mut checked_a_later_chunk = false;
    for (baseline, mutated) in metadata_result.chunks.iter().zip(content_result.chunks.iter()) {
        if baseline.metadata.byte_start == 0 {
            // The first chunk starts with the literal "# Setup" heading line, whose
            // exact transformed shape depends on the heading-stripping logic this
            // test intentionally does not re-derive. Every later chunk below already
            // proves the breadcrumb is written into `content`.
            continue;
        }

        assert_eq!(
            baseline.metadata.heading_path,
            vec!["Setup".to_string()],
            "every chunk must resolve heading_context from the single leading heading"
        );
        assert!(
            !baseline.content.starts_with('#'),
            "a chunk starting after byte 0 cannot contain the document's only '#', got: {:?}",
            baseline.content
        );

        let expected = format!("# Setup\n\n{}", baseline.content);
        assert_eq!(
            mutated.content, expected,
            "Content target must prepend the breadcrumb ahead of the untouched chunk body"
        );
        assert_eq!(mutated.metadata.heading_path, vec!["Setup".to_string()]);
        checked_a_later_chunk = true;
    }
    assert!(
        checked_a_later_chunk,
        "test fixture must produce at least one chunk starting after the heading line"
    );
}

/// `heading_path` is populated under BOTH targets — that is the whole reason there is
/// no `Both` variant. `Metadata` does not withhold the breadcrumb, it only keeps it out
/// of `content`, so a lexical index can be built from `content` directly while a dense
/// one still has the section context available from metadata.
#[test]
fn heading_path_is_populated_under_every_target() {
    let content_result = chunk_for_rag(MARKDOWN, &config(BreadcrumbTarget::Content)).unwrap();
    let metadata_result = chunk_for_rag(MARKDOWN, &config(BreadcrumbTarget::Metadata)).unwrap();

    assert_eq!(content_result.chunks.len(), metadata_result.chunks.len());

    let mut checked_a_chunk_under_a_heading = false;
    for (content_chunk, metadata_chunk) in content_result.chunks.iter().zip(metadata_result.chunks.iter()) {
        assert_eq!(
            metadata_chunk.metadata.heading_path, content_chunk.metadata.heading_path,
            "heading_path must be identical under both targets; only `content` differs"
        );
        if !content_chunk.metadata.heading_path.is_empty() {
            checked_a_chunk_under_a_heading = true;
        }
    }
    assert!(
        checked_a_chunk_under_a_heading,
        "test fixture must produce at least one chunk carrying a heading_path"
    );
}

#[test]
fn breadcrumb_target_is_inert_when_prepend_heading_context_disabled() {
    let mut baseline_config = config(BreadcrumbTarget::Content);
    baseline_config.prepend_heading_context = false;
    let baseline_result = chunk_for_rag(MARKDOWN, &baseline_config).unwrap();

    for target in [BreadcrumbTarget::Content, BreadcrumbTarget::Metadata] {
        let mut disabled_config = baseline_config.clone();
        disabled_config.breadcrumb_target = target;
        let result = chunk_for_rag(MARKDOWN, &disabled_config).unwrap();

        assert_eq!(result.chunks.len(), baseline_result.chunks.len());
        for (baseline_chunk, chunk) in baseline_result.chunks.iter().zip(result.chunks.iter()) {
            assert_eq!(
                chunk.content, baseline_chunk.content,
                "breadcrumb_target ({target:?}) must be inert while prepend_heading_context is false"
            );
        }
    }
}

#[test]
fn breadcrumb_target_serde_round_trips_snake_case() {
    for (value, json) in [
        (BreadcrumbTarget::Content, "\"content\""),
        (BreadcrumbTarget::Metadata, "\"metadata\""),
    ] {
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, json);
        let deserialized: BreadcrumbTarget = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized, value);
    }
}
