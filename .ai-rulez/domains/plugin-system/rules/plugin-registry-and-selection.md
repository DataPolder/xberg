---
priority: critical
---

- Separate typed registry per plugin type — eight process-global statics in `plugins/registry/mod.rs`
- Thread safety: `Arc<parking_lot::RwLock<_>>` for all registries. Guards are not poisoned; `.read()`/`.write()` return the guard directly, not a `Result`.
- Priority is `i32`, default 50. Negative values are representable and nothing clamps to 255. The documented bands (`plugins/extractor/trait.rs`) are 0-25 fallback, 26-49 alternative, 50 default, 51-75 premium, 76-100 specialized — a convention, not an enforced range.
- Selection: highest priority plugin matching the MIME type wins (`priority_map.iter().next_back()`)
- Lookup is `HashMap<mime, BTreeMap<priority, entry>>`: an exact MIME hit is O(1) and the BTreeMap orders by priority, not by MIME. The wildcard-family path is a linear scan over every registered MIME, so worst case is O(n).
- Equal (MIME type, priority) is a COLLISION, not a tie-break: the later registration displaces the earlier one, logs a warning, and prunes the displaced plugin's name-index entry. Last registration wins regardless of language — give competing plugins distinct priorities.
- Dynamic registration: plugins can be added/removed at runtime (`register`/`remove`/`clear`/`shutdown_all`)
- Validate plugin before registration (check capabilities, supported formats); registration calls `initialize()` and a failing `initialize()` blocks it
