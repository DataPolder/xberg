---
priority: high
---

- Track confidence scores on all OCR results — expose in API. Confidence is NOT comparable across backends: ask the backend via `OcrBackend::confidence_semantics` (`plugins/ocr.rs`) and never threshold an `Uncalibrated` score against a Tesseract-derived number.
- `font_size` on an OCR element is TWO different quantities. Tesseract reports TYPOGRAPHY (hOCR `x_fsize`, `pdf/structure/adapters.rs::hocr_font_size_pt`); sceptre and paddle report DETECTION-BOX GEOMETRY (`geometric_ocr_font_size_pt`). Never compare or threshold them alike. Both fall back to `DEFAULT_OCR_FONT_SIZE_PT = 12.0`.
- Tesseract emits `x_fsize` only because `tesseract_variable_set` sets `hocr_font_info=1` unconditionally (`ocr/processor/config.rs`). Without it every font size is the 12.0 fallback and heading clustering has no signal.
- The geometric proxy is what heading detection on sceptre/paddle rests on, and it is fragile: a skewed-AABB bug once made resolved font sizes track word count, so every page logged `headings=0` on a document where Tesseract found 61 (`adapters.rs`, `test_ocr_doc_uses_quad_edge_height_not_skewed_aabb_height_for_font_size`). Sceptre and paddle elements also never carry the hOCR bold/italic fractions, so style flags fall back to `(false, false)`.
- Backends silently drop whole pages. `accept_or_reject_ocr_page` (`extractors/pdf/ocr.rs`) vetoes a page on fragmented-word ratio and, when reported, dictionary-invalid ratio; a rejected page's structured paragraphs are discarded too. A higher raw word count from one backend is therefore NOT evidence of better recall — compare accepted-page counts first.
- `dict_invalid_word_ratio` is `None` for every non-Tesseract backend and for pages too short to measure. Absence is never 0.0.
- On this corpus PRECISION is the scarce resource, not recall. A recall-improving change must clear a ground-truth F1 A/B before landing: a detached-marker rewrite went 50.39 -> 47.64 GT F1 with zero files improved and was rejected on exactly that (`adapters.rs`, `DETACHED_MARKER_RUN_MIN_LENGTH`).
- Measure structure (headings, lists) on Markdown, never on Plain. `rendering/plain.rs` renders no list marker glyph at all, so a Plain measurement cannot distinguish "never detected" from "detected then normalised away".
- Image preprocessing defaults are narrower than the name suggests: `ImagePreprocessingConfig::default()` (`types/formats.rs`) is `target_dpi 300`, `deskew: true`, `binarization_method: "otsu"`, with `auto_rotate`, `denoise`, `contrast_enhance`, and `invert_colors` all OFF. No A/B in this repo measures the accuracy delta of any of them.
- PSM mode selection: auto-detect layout, allow user override (single block, single line, sparse text, etc.). Native default is PSM 3 (auto); WASM default is PSM 6 (single block).
- Language detection: validate requested languages are available, provide install hints if not
- Multi-language support: allow multiple languages per OCR request
- Test OCR accuracy against ground-truth documents via the Benchmarks workflow (`MEASURE_QUALITY`, `GROUND_TRUTH_DIR`). It is `workflow_dispatch` only — nothing gates a merge on OCR accuracy.
