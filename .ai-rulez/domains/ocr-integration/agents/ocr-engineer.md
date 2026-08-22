---
name: ocr-engineer
description: OCR pipeline development, backend integration, and table reconstruction
model: sonnet
---

When working on OCR code:

1. Key source paths: `crates/xberg/src/ocr/` — `processor/` (dir), `tesseract_backend.rs`, `hocr_parser.rs`, `cache.rs`, `validation.rs`, `tessdata_manager.rs`, `tessdata_download.rs`, `table/`, `layout_assembly.rs`. Also `crates/xberg/src/table_core.rs` (grid reconstruction), `crates/xberg/src/pdf/structure/adapters.rs` (OCR → document structure), `crates/xberg/src/extractors/pdf/ocr.rs` (the scanned-PDF route), `crates/xberg/src/sceptre_ocr/`, `crates/xberg/src/paddle_ocr/`, `crates/xberg-tesseract/` (the C FFI crate).
2. The OCR pipeline: Image Detection -> Preprocessing (deskew + otsu by default; denoise/contrast/auto-rotate are OFF) -> Backend Selection -> OCR Execution -> hOCR Parsing -> Table Reconstruction -> Page Accept/Reject -> Caching -> Return
3. Backends: Tesseract (default, C FFI via the in-repo `crates/xberg-tesseract` — there is no `leptess`), PaddleOCR (ONNX via ort), sceptre (CRAFT+CRNN, selected by name through `OcrBackendType::Custom`), Candle OCR, VLM OCR, and custom plugin backends.
4. For plugin or external-process backends: use tokio::task::spawn_blocking for blocking work, minimize FFI/runtime lock hold time, cache backend data in Rust fields
5. For table detection: cluster word bboxes — `detect_rows` by y-centre, then `merge_words_into_cell_tokens`, then `detect_columns` on the merged tokens. Never run `detect_columns` on raw words (multi-word cells mint spurious columns and the validator rejects every row). Cells are never re-OCR'd. Output markdown.
6. For language management: validate ISO 639 codes and tessdata availability. Note `TesseractConfig` exists twice (`types::formats` public, `ocr::types` internal) with independent `Default` impls — change both or standalone image OCR keeps the stale value.
7. OCR cache key = image hash + backend + config hash + output format; the config hash folds in `TESSERACT_RESULT_SCHEMA_VERSION` and the whole ordered `tesseract_variable_set`. It contains no build id, so bump `TESSERACT_RESULT_SCHEMA_VERSION` — or disable the cache — before any A/B or revert-check, else the arm replays the control's output.
8. hOCR parsing: `ocr/hocr_parser.rs` extracts word-level bounding boxes and confidence scores. `x_fsize` only exists because `tesseract_variable_set` sets `hocr_font_info=1`; without it every font size is the 12.0 fallback.
9. `font_size` is two different quantities: Tesseract reports typography (`hocr_font_size_pt`), sceptre/paddle report detection-box geometry (`geometric_ocr_font_size_pt`). Confidence likewise is not comparable across backends — check `ConfidenceSemantics` before gating on it.
10. The OCR raster is MediaBox-oriented on purpose (the renderer applies `/Rotate`, `normalize_rendered_page_for_ocr` undoes it). OCR segments hardcode `rotation_degrees: 0.0`, so anything reasoning about geometry on a `/Rotate` page must thread the page rotation itself (`DetachedMarkerFrame::OcrOnPage`).
11. Backends silently drop pages via `accept_or_reject_ocr_page`. Compare accepted-page counts before comparing word counts. Precision is the scarce resource on this corpus — clear a GT-F1 A/B before landing a recall-improving change. Measure headings/lists on Markdown, never on Plain (Plain renders no list marker glyph).
