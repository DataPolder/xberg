//! Regression test for issue #279: `show_download_progress` has zero readers on all four
//! model-download configs (`EmbeddingConfig`, `SparseEmbeddingConfig`, `RerankerConfig`,
//! `LateInteractionConfig`) — no progress reporting exists in any model-download path for
//! any of them.
//!
//! Worse, the `Plugin`-variant doc comments on `EmbeddingModelType` and `RerankerModelType`
//! stated the field is ignored *only* for the `Plugin` variant, which implied it works for
//! every other variant. It never worked for any variant. This test asserts the doc comments
//! were corrected to state the truth, by inspecting the source text directly (a pure
//! documentation fix has no runtime behavior to assert against).

const PROCESSING_SRC: &str = include_str!("../src/core/config/processing.rs");
const SPARSE_EMBEDDING_SRC: &str = include_str!("../src/core/config/sparse_embedding.rs");
const RERANKER_SRC: &str = include_str!("../src/core/config/reranker.rs");
const LATE_INTERACTION_SRC: &str = include_str!("../src/core/config/late_interaction.rs");

/// Each `show_download_progress` field doc must disclose that the field currently has no
/// effect for any model variant, not just claim what it's "reserved" for.
#[test]
fn all_four_show_download_progress_field_docs_disclose_no_effect() {
    for (name, src) in [
        ("EmbeddingConfig", PROCESSING_SRC),
        ("SparseEmbeddingConfig", SPARSE_EMBEDDING_SRC),
        ("RerankerConfig", RERANKER_SRC),
        ("LateInteractionConfig", LATE_INTERACTION_SRC),
    ] {
        let field_doc = extract_field_doc(src, "pub show_download_progress: bool,")
            .unwrap_or_else(|| panic!("{name}: could not locate show_download_progress field"));
        assert!(
            field_doc.contains("no effect for any") && field_doc.contains("(#279)"),
            "{name}: show_download_progress doc must disclose it has no effect for any model \
             variant, got:\n{field_doc}"
        );
    }
}

/// Before the fix, `EmbeddingModelType::Plugin`'s doc comment listed `show_download_progress`
/// alongside genuinely Plugin-specific-ignored fields (`batch_size`, `cache_dir`,
/// `acceleration`), implying it works normally for `Preset`/`Custom`/`Llm`. It must now be
/// called out separately as globally inert.
#[test]
fn embedding_model_type_plugin_doc_does_not_imply_show_download_progress_works_elsewhere() {
    let doc = extract_variant_doc(PROCESSING_SRC, "Plugin {\n        /// Name the backend was registered")
        .expect("could not locate EmbeddingModelType::Plugin doc");
    assert!(
        !doc.contains("`show_download_progress`, `acceleration`"),
        "show_download_progress must no longer be lumped in with the Plugin-only-ignored \
         fields, got:\n{doc}"
    );
    assert!(
        doc.contains("no effect for any variant"),
        "Plugin doc must clarify show_download_progress is globally inert, got:\n{doc}"
    );
}

/// Same check for `RerankerModelType::Plugin`.
#[test]
fn reranker_model_type_plugin_doc_does_not_imply_show_download_progress_works_elsewhere() {
    let doc = extract_variant_doc(RERANKER_SRC, "Plugin {\n        /// Name the backend was registered")
        .expect("could not locate RerankerModelType::Plugin doc");
    assert!(
        !doc.contains("`show_download_progress`,"),
        "show_download_progress must no longer be lumped in with the Plugin-only-ignored \
         fields, got:\n{doc}"
    );
    assert!(
        doc.contains("no effect for any variant"),
        "Plugin doc must clarify show_download_progress is globally inert, got:\n{doc}"
    );
}

/// Extract the contiguous `///` doc-comment block immediately preceding `field_decl`.
fn extract_field_doc(src: &str, field_decl: &str) -> Option<String> {
    let field_pos = src.find(field_decl)?;
    let before = &src[..field_pos];
    let mut lines: Vec<&str> = Vec::new();
    for line in before.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            lines.push(trimmed);
        } else if trimmed.is_empty() || trimmed.starts_with("#[serde") {
            continue;
        } else {
            break;
        }
    }
    lines.reverse();
    Some(lines.join("\n"))
}

/// Extract the contiguous `///` doc-comment block immediately preceding `variant_marker`
/// (the variant's opening line, used as a unique anchor since variant names alone repeat
/// across enums in this file).
fn extract_variant_doc(src: &str, variant_marker: &str) -> Option<String> {
    let variant_pos = src.find(variant_marker)?;
    let before = &src[..variant_pos];
    let mut lines: Vec<&str> = Vec::new();
    for line in before.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            lines.push(trimmed);
        } else if trimmed.is_empty() {
            continue;
        } else {
            break;
        }
    }
    lines.reverse();
    Some(lines.join("\n"))
}
