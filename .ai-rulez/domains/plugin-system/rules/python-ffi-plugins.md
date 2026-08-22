---
priority: high
---

- The Python bridges are ALEF-GENERATED into `crates/xberg-py/src/lib.rs` (there is no `crates/xberg-py/src/plugins.rs`). Do not hand-write them — change the generator.
- GIL management: pyo3 0.29, so `Python::attach`, not `Python::with_gil`. No call site uses `allow_threads` and none ever has.
- Cache frequently-accessed Python data in Rust fields (e.g. `cached_name`) so infallible trait methods need no GIL acquisition.
- Use `tokio::task::spawn_blocking` for async calls to Python backends, and propagate the caller's `contextvars` context.
- The bridge marshals EVERY trait method's return value, not just payload-carrying enums: a native `extract::<T>()` fast path falling back to a `json.dumps` round trip. Any type crossing the bridge therefore needs `Serialize + Deserialize` AND `Default` — including unit-only enums (see `plugins::ocr::PageOrientationHandling`).
- Host exceptions become `XbergError::Other` carrying the plugin name and method. The Python exception type and traceback are not preserved, and no `XbergError::Plugin` is constructed on this path.
- Infallible trait methods cannot propagate a host exception: they `tracing::warn!` and substitute `Default::default()`. A silent default is indistinguishable from a real result — never treat one as data.
- Validate Python plugin protocol compliance on registration
- No benchmark in this repo measures GIL acquisition cost. Do not quote a per-acquisition target.
