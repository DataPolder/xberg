---
title: "Changelog"
---

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [1.0.0] - 2026-07-27

xberg 1.0.0 is the first stable release of the document-intelligence engine previously developed as
**Kreuzberg**. It is the direct successor to Kreuzberg v4 and shares the same Rust core and extraction
API lineage — v5 is the Xberg-branded line. The Kreuzberg v4 line continues as LTS at
[kreuzberg-dev/kreuzberg-lts](https://github.com/kreuzberg-dev/kreuzberg-lts). This entry summarizes
what changed relative to Kreuzberg v4.9.

For a step-by-step upgrade, see the [migration guide](/migration/from-kreuzberg-v4/).

### Migration from Kreuzberg v4

- Packages are renamed `kreuzberg` → `xberg` across every ecosystem (crates.io, PyPI, npm, Maven,
  NuGet, Composer, RubyGems, Hex, Go).
- The Rust error type `KreuzbergError` is now `XbergError`.
- Environment variables are re-prefixed `KREUZBERG_*` → `XBERG_*`, and config files are discovered as
  `xberg.{toml,yaml,yml,json}`.
- The **R binding** and the **EasyOCR** backend are removed (see Removed). Existing Kreuzberg v4
  installs keep working under their original names.

The full identifier mapping is in the [migration guide](/migration/from-kreuzberg-v4/).

### Added

- **Candle OCR backend.** A pure-Rust, CPU-only OCR backend (TrOCR / GLM-OCR / Hunyuan-OCR) that needs
  no ONNX Runtime or native Tesseract.
- **Audio and video transcription.** Whisper-based transcription extracts text from `.mp3`, `.wav`,
  `.m4a`, `.mp4`, and `.webm`.
- **Named-entity recognition.** GLiNER2-based entity extraction, including an in-browser WASM
  `NerModel` that detects entities locally with no server round-trip.
- **Retrieval building blocks.** Sparse embeddings (SPLADE), ColBERT late-interaction retrieval, and a
  reranking / semantic-search stage alongside the existing dense embeddings.
- **Text intelligence.** Redaction with reversible rehydration, summarization, translation, VLM image
  captioning, QR-code detection, document diffing, and page/chunk classification.
- **New document formats (98 total).** WordPerfect `.wpd`/`.wp`/`.wp5`, HEIC/HEIF/AVIF images, and the
  audio/video formats above.
- **Four new language bindings.** Dart/Flutter, Swift, Kotlin/Android, and Zig — for 15 language
  bindings over one engine.
- **Wider code intelligence.** tree-sitter coverage grows from 248 to 306 programming languages.

### Changed

- **Renamed from Kreuzberg to xberg** across packages, namespaces, and the Rust `KreuzbergError` →
  `XbergError` type (see Migration).
- **Environment variables** use the `XBERG_` prefix; new layout, OCR model-tier, CoreML, and ORT
  execution-provider variables are available.
- **Config discovery** now also accepts the `.yml` extension (`xberg.{toml,yaml,yml,json}`).
- **Models and cache** live under the `xberg` cache segment and the `xberg-io` Hugging Face org; the
  project domain is `xberg.io`.
- **License.** Relative to the Kreuzberg 4.8/4.9 line (Elastic License 2.0), xberg 1.0.0 is **MIT**.

### Removed

- **R binding** — the Kreuzberg v4 LTS line is the last to ship it.
- **EasyOCR backend** — the Python/torch-only backend is dropped; use Tesseract, PaddleOCR, Candle, or
  a VLM backend instead.
- **Bundled `pdfium-render` fork** and its `KREUZBERG_PDFIUM_BUNDLED_PATH` variable, and the standalone
  `@kreuzberg/core` npm package.

### Packaging

- Distribution hardening across all 15 targets: ONNX Runtime bundling, glibc/musl floors, NuGet
  runtime-package size splits, Homebrew bottles, Go module tags, Swift C++ linkage, and Zig release
  uploads. Published to crates.io, PyPI, npm, Maven Central, NuGet, RubyGems, Packagist, Hex, pub.dev,
  Go, Swift Package Manager, Homebrew, Docker (`ghcr.io/xberg-io/xberg`), and a Helm chart.
