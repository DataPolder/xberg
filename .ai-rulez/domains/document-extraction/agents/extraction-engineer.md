---
name: extraction-engineer
description: Document extraction pipeline development and maintenance
model: sonnet
---

When working on document extraction code:

1. Key source paths: `crates/xberg/src/core/` — `mime.rs`, and the directories `extractor/` (batch.rs, bytes.rs, file.rs), `extract/`, `config/`, `pipeline/`. Extractor implementations live in `crates/xberg/src/extractors/` and `crates/xberg/src/extraction/`; the plugin traits in `crates/xberg/src/plugins/`.
2. The extraction pipeline: Input -> Cache Check -> MIME Detection -> Format Conversion -> Extractor Selection (priority-based) -> Extraction -> Fallback Chain -> Post-Processing -> Caching -> Output
3. For MIME detection: `EXT_TO_MIME` map is consulted first, `infer` magic bytes are the fallback. Always `validate_mime_type()` before extraction.
4. For caching: key is `<cache_version_tag>-<content_hash>-<config_hash>`. The tag comes from crate version + `CACHE_SCHEMA_VERSION` (`cache/version.rs`) and is NOT a build fingerprint — two builds at the same crate version share entries. Bump `CACHE_SCHEMA_VERSION` for any behaviour change, and before any A/B or revert-check, or the experiment replays the control's output.
5. For errors: fallback runs only when `is_extractor_fallback_eligible` says so — `UnsupportedFormat` and `Plugin` only (`core/extractor/file.rs`). Parse/IO/OCR/Validation errors abort the chain deliberately. Preserve partial results where the extractor can. `XbergError` variants are `{ message, #[source] source }`; there is no `suggestion` field.
6. For new formats: add to `EXT_TO_MIME`, implement the `DocumentExtractor` trait (`async fn extract(&self, input: ExtractInput, config: &ExtractionConfig) -> Result<ExtractedDocument>`), register in `register_default_extractors()` (`crates/xberg/src/extractors/mod.rs`, `pub(crate)`). Update the format headline test in `core/mime.rs` in the same change.
7. Always use `SecurityLimits` validators for user content (`ZipBombValidator`; `DepthValidator` and `StringGrowthValidator` are `pub(crate)`, usable inside the crate only). Set `max_pages` for anything running a per-page OCR/layout pipeline — byte caps do not bound page count.
8. Run `task test` after changes. No coverage gate exists in CI; verify by running the tests that cover the change.
