---
description: Plugin trait system and Python FFI integration
---

- Base trait `Plugin` (`plugins/traits.rs`): `Send + Sync`, with `name()`, `version()`, `initialize()`, `shutdown()`. Subtraits add the work method — `DocumentExtractor::extract`, `OcrBackend::process_image`, and so on. There is no `Extractor` or `MetadataExtractor` trait.
- Eight subtraits and eight typed registries: DocumentExtractor, OcrBackend, PostProcessor, Validator, EmbeddingBackend, RerankerBackend, TokenizerBackend, Renderer (`plugins/registry/mod.rs`). There is no single `PluginRegistry` type.
- Discovery: static registration (Rust plugins compiled in, `register_default_extractors()`) + dynamic runtime registration from the language bindings. No hot-reload exists for any plugin type.
- Priority selection: plugins declare an `i32` priority per MIME type; the registry selects the highest-priority match. Equal (MIME, priority) is a collision — the later registration displaces the earlier one and warns.
- Python FFI: Python plugins implement a class matching the trait interface, bridged through ALEF-generated code in `crates/xberg-py/src/lib.rs` (no `plugins.rs` file exists). Every trait return value is marshalled, via a native `extract::<T>()` fast path or a `json.dumps` round trip.
- GIL management: `Python::attach` inside `tokio::task::spawn_blocking`, with frequently-read data cached in Rust fields to avoid acquisition. `allow_threads` is not used anywhere.
- Plugin lifecycle: `initialize()` at registration (it validates dependencies and a failure blocks registration) → work method → `shutdown()`. There is no separate validate or ready state.
- Error handling: `XbergError::Plugin { .. }` exists for Rust-side plugin failures and is one of only two fallback-eligible extraction errors. The Python bridge does NOT use it — host exceptions become `XbergError::Other` with the plugin name and method, and infallible methods warn and return `Default::default()`. There is no `PluginError` type.
- Testing: test doubles (`MockExtractor`, `FailingExtractor`) are the norm for registry-level unit tests; use real backends for integration tests. Test fallback chains and Python plugin loading/unloading.
