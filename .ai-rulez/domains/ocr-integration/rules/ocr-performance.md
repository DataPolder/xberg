---
priority: high
---

- OCR cache key = `image_hash + backend + config_hash + output_format` (`ocr/cache.rs::generate_cache_key`). The config hash folds in `TESSERACT_RESULT_SCHEMA_VERSION` and the full ordered `tesseract_variable_set(config)` (`ocr/processor/config.rs`).
- The key contains NO build id and NO code hash. Changing OCR behaviour without changing a hashed input replays the previous result — which silently voids revert-checks and A/B comparisons. Bump `TESSERACT_RESULT_SCHEMA_VERSION` whenever output can change for an unchanged image and config. (#687: `hocr_font_info` landed outside the key and 306 stale entries kept being served.)
- Invalidate cache when OCR config changes (backend, language, PSM mode)
- Batch processing: process multiple images concurrently with configurable parallelism
- Resource management: limit concurrent OCR operations to avoid memory exhaustion (`sceptre_ocr` caps reader and execution slots; `resolve_thread_budget` caps threads by cgroup quota)
- No benchmark measures OCR wall-clock per page, and no CI job gates on one. Do not treat any per-page latency figure as a documented target — `mcp/server.rs` already cites an unmeasured one as justification for `EXTRACTION_TASK_TTL_MS`.
- Monitor and log OCR processing times for regression detection
