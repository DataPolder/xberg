---
priority: high
---

- Cache keys: `<cache_version_tag>-<content_hash>-<config_hash>`, never path-based. The tag is derived from `CARGO_PKG_VERSION` and `CACHE_SCHEMA_VERSION` only (`cache/version.rs`).
- The tag is NOT a build fingerprint — no git SHA, no build id, no timestamp. Two separately built binaries at the same crate version and schema version produce the same tag and SHARE cache entries.
- A behaviour change not paired with a crate version bump MUST bump `CACHE_SCHEMA_VERSION`, or entries encoding the old behaviour outlive the fix.
- An A/B or revert-check of a code change MUST bump the schema version or disable the cache first. Otherwise the experiment arm replays the control's output and comes back byte-identical for that reason alone.
- Invalidate cache when extraction config changes (output format, OCR settings, etc.)
- Check cache before any extraction — cache hits should skip all processing
- Concurrent batch processing: use a configurable worker pool defaulting to `num_cpus::get()` capped by any detected Linux cgroup CPU quota (`core/config/concurrency.rs::resolve_thread_budget`). In a container the default is the quota, not the host core count.
- Large files are read whole into memory today (`core/io.rs::read_file_async` is `tokio::fs::read`). There is no `AsyncRead` streaming path — treat streaming as an open gap, not an existing facility.
- Cache hits/misses are counted as OTel metrics (`cache/core.rs`, `ocr/cache.rs`); no hit-rate ratio is computed or asserted anywhere.
