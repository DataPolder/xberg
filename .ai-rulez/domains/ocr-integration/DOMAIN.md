---
description: OCR backend integration and image processing
---

- Multiple backends: Tesseract (C FFI via the in-repo `crates/xberg-tesseract`, default), PaddleOCR (ONNX Runtime), sceptre (CRAFT + CRNN over ORT/tract/candle, selected by name through the `Custom` variant), Candle OCR, VLM OCR, and custom plugin backends
- Backend selection: priority-based with fallback — Tesseract default, PaddleOCR for CJK, plugin/VLM backends as configured
- Image preprocessing is available but mostly OFF by default: only `deskew` and otsu binarization run unless configured. `auto_rotate`, `denoise`, `contrast_enhance`, and `invert_colors` all default to false.
- PSM modes: configurable page segmentation (single block, single line, sparse text) per use case; native default PSM 3, WASM default PSM 6
- Table detection: word-bbox clustering (rows by y-centre, columns by left edge after word→cell-token merging) → grid validation → Markdown table output. No line/intersection detection, no per-cell re-OCR.
- hOCR: parse Tesseract hOCR output for word-level bounding boxes, confidence scores, reading order. Only the Tesseract path writes `x_fsize` and the bold/italic fractions; geometric backends carry none of them.
- The OCR raster is MediaBox-oriented BY DESIGN: the renderer applies `/Rotate`, then `normalize_rendered_page_for_ocr` rotates back to user space, so OCR pixel axes already align with the MediaBox and `pixel_bbox_to_pdf_points` is a pure scale plus y-flip.
- OCR segments hardcode `rotation_degrees: 0.0` and cannot carry a per-segment rotation the way native PDF text does. Anything comparing OCR geometry on a `/Rotate` page must thread page rotation explicitly (`DetachedMarkerFrame::OcrOnPage`).
- Language management: the user specifies the OCR language and it is validated against available tessdata. There is no feedback loop from language detection to traineddata selection — `language_detection` runs post-extraction on the extracted text (`core/pipeline/features.rs`).
- Caching: OCR results are keyed by image hash + backend + config hash + output format; the config hash covers the full ordered Tesseract variable set. No build id — see the ocr-performance rule.
- Confidence tracking: per-word and per-page confidence scores, but the page-level number's meaning is backend-specific (`ConfidenceSemantics`). Pages can be vetoed outright by `accept_or_reject_ocr_page`.
