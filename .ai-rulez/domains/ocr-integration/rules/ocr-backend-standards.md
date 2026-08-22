---
priority: critical
---

- Pluggable backend architecture: all backends implement the OcrBackend trait
- Backend independence: switching backends must not require API changes
- Tesseract is the default backend, bound through the in-repo `crates/xberg-tesseract` C FFI crate (`links = "xberg_tesseract"`, builds Tesseract from source under the `build-tesseract` feature). There is no `leptess` dependency.
- `OcrBackendType` is `Tesseract | PaddleOCR | Candle | Custom` (`plugins/ocr.rs`). Sceptre (`crates/xberg/src/sceptre_ocr/`, CRAFT detection + CRNN recognition) routes through `Custom` by name — it is not its own variant.
- Blocking OCR backends: use tokio::task::spawn_blocking and keep runtime/FFI locks held only around the blocking call
- Graceful degradation: if preferred backend unavailable, fall back to next available
- Backends report confidence on incompatible scales. Every backend must declare `OcrBackend::confidence_semantics` — `Legibility { scale_max }`, `Uncalibrated` (the default), or `None`. Never gate on an `Uncalibrated` score: a Tesseract-calibrated gate applied to sceptre once rejected every page of a 16-page document.
- Rotated-page handling is a per-backend capability, not a guarantee — see `PageOrientationHandling` in `plugins/ocr.rs` before assuming a backend copes with a sideways raster.
- Document installation requirements and troubleshooting for each backend
