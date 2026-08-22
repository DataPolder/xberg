---
priority: high
---

- Mock plugin testing: create test doubles for unit tests
- Real plugin testing: integration tests with actual backends
- Thread safety tests: run concurrent plugin operations to detect race conditions
- No benchmark measures plugin dispatch overhead against a direct call. If you need that number, write the bench — do not assume one exists.
- Test all error paths: invalid input, backend failure, timeout, resource exhaustion
- Test plugin lifecycle: register, use, unregister, verify cleanup
