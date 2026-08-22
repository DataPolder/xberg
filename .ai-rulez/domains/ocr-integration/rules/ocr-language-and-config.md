---
priority: medium
---

- Validate language packs exist before OCR execution — fail fast with helpful message (`ocr/validation.rs`, `ocr/tessdata_manager.rs`, `ocr/tessdata_download.rs`)
- Support ISO 639 language codes (639-1 and 639-3), map to backend-specific formats (`ocr/validation.rs::validate_language_code`)
- `TesseractConfig` exists TWICE with independent `Default` impls: the public `types::formats::TesseractConfig` and the internal `ocr::types::TesseractConfig`. Several `extractors::image` call sites build the PUBLIC default and convert it before the internal default is ever consulted — so a default changed in only one place reaches PDF OCR but not standalone image OCR. Change both and keep the sync test in `types/formats.rs` passing.
- Configuration cascade differs by mode — see the config-loading-precedence skill. CLI mode: individual flags > inline JSON > config file > env > defaults. Server/MCP mode has its own order.
- Provide troubleshooting guides for common issues (missing tessdata, backend not found)
- Language pack installation: document per-platform instructions
