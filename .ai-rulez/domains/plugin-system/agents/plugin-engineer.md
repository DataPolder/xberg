---
name: plugin-engineer
description: Plugin system architecture, registry management, and Python FFI
model: sonnet
---

When working on the plugin system:

1. Key source paths: `crates/xberg/src/plugins/` — `mod.rs`, `traits.rs`, `ocr.rs`, `embedding.rs`, `reranker.rs`, `tokenizer.rs`, `renderer.rs`, `startup_validation.rs`, and the directories `extractor/`, `processor/`, `validator/`, `registry/`. The Python bridge is ALEF-generated into `crates/xberg-py/src/lib.rs`; there is no `crates/xberg-py/src/plugins.rs`.
2. Eight plugin types, all extending `Plugin` (`Send + Sync`): DocumentExtractor, OcrBackend, PostProcessor, Validator, EmbeddingBackend, RerankerBackend, TokenizerBackend, Renderer. Lifecycle is `initialize()` → work method → `shutdown()`.
3. Priority is `i32`, default 50, higher wins per MIME type. The 0-100 bands in `plugins/extractor/trait.rs` are convention only — nothing clamps the range. Equal (MIME, priority) is a collision: the later registration displaces the earlier and logs a warning.
4. Registries are `Arc<parking_lot::RwLock<_>>` over `HashMap<mime, BTreeMap<priority, entry>>` — exact MIME lookup is O(1), the wildcard-family path is a linear scan.
5. Python plugins: validate protocol compliance; `Python::attach` (pyo3 0.29) inside `tokio::task::spawn_blocking`. `allow_threads` is not used in this repo.
6. For new plugin types: define trait extending Plugin, create typed registry, add registration functions, implement priority-based selection. Every type crossing the Python bridge needs `Serialize + Deserialize + Default` — the bridge marshals every trait return value, unit-only enums included.
7. GIL optimization: cache frequently-accessed Python data in Rust fields. No GIL-overhead benchmark exists — do not cite a target figure.
8. All plugins must handle errors gracefully — return Result, never panic. Rust-side plugin failures surface as `XbergError::Plugin` (fallback-eligible in the extractor chain); host exceptions from the Python bridge become `XbergError::Other`, and infallible methods warn and substitute `Default::default()`.
