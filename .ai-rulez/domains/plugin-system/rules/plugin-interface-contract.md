---
priority: critical
---

- All plugins must implement the base `Plugin` trait (`plugins/traits.rs`): `Send + Sync`. (`'static` is not in the bound; it arrives implicitly via `Arc<dyn Trait>` at the registries.)
- Eight plugin types, each with its own typed registry: `DocumentExtractor`, `OcrBackend`, `PostProcessor`, `Validator`, `EmbeddingBackend`, `RerankerBackend`, `TokenizerBackend`, `Renderer`. All eight are externally registrable from the language bindings.
- Async execution: use async trait methods for non-blocking operations
- Lifecycle: `initialize()` -> the subtrait's work method (`extract`, `process_image`, …) -> `shutdown()`. There is no `process()` on `Plugin`. `initialize()` must validate all requirements; a plugin whose `initialize()` errors is not registered.
- Never panic in plugin code — all errors must be returned as Result
- Extractors return `ExtractedDocument` from `async fn extract(&self, input: ExtractInput, config: &ExtractionConfig)`. There is no `ExtractionResult` type.
