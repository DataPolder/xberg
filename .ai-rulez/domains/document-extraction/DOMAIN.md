---
description: Document extraction pipeline architecture
---

- Pipeline: file input → cache check → MIME detection (`EXT_TO_MIME` extension map first, `infer` magic-byte fallback) → extractor routing → extraction → post-processing → `ExtractedDocument`
- Extractors are plugins implementing `DocumentExtractor` (`plugins/extractor/trait.rs`): `async fn extract(&self, input: ExtractInput, config: &ExtractionConfig) -> Result<ExtractedDocument>`. There is no `Extractor` trait and no `ExtractionResult` type. (`ExtractionSource` is a service-layer request enum in `service/request.rs`, unrelated to the extractor trait.)
- Fallback runs only for `UnsupportedFormat` and `Plugin` errors (`is_extractor_fallback_eligible`); a parse/IO/OCR error aborts the chain.
- OCR is a stage INSIDE `PdfExtractor`, not a lower-priority fallback extractor: exactly one extractor claims `application/pdf` (asserted in `extractors/pdf/mod.rs`). Do not model PDF as "native → OCR extractor → error".
- Cache-first: check the extraction cache before running extractors; key is `<cache_version_tag>-<content_hash>-<config_hash>` — see the cache-and-performance rule for what the tag does and does not distinguish.
- `ExtractedDocument` contains: text content, metadata (page count, language, confidence), optional structured data (tables, images)
- Async-first: all extraction paths are async, use spawn_blocking for CPU-bound work (OCR, image processing)
- Memory limits: `SecurityLimits` caps archive size, content size, nesting depth, entity length, table cells, and (opt-in) page count. There is no input-file size cap — no `max_file_size` exists — and no streaming reader; files are read whole.
- Format coverage: 100 formats (120 file extensions) — PDF, DOCX, XLSX, PPTX, HTML, images (incl. HEIC/HEIF/AVIF), email (EML/MSG), archives, plain text, WordPerfect (WPD). Asserted by `core::mime::tests::format_and_extension_counts_match_the_published_headline`; change it only together with that test and the copy it lists.
