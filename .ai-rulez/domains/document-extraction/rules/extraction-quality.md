---
priority: high
---

- Cover core extraction code and bindings thoroughly. There is no coverage gate: `task test:cov` renders a report and nothing consumes it, and no CI job checks a threshold — do not quote a coverage percentage as a contract.
- Test all format categories: text, office, PDF, images, archives, markup
- Test corrupted/malformed documents — extraction must fail gracefully, never panic
- Benchmark extraction speeds per format via the Benchmarks workflow (`.github/workflows/benchmarks.yaml`). It is `workflow_dispatch` only — it does not run on push or PR and gates no merge, so a regression is caught only when someone dispatches it.
- Test both success and error paths for every extractor
- The format headline is the one enforced number: `core::mime::tests::format_and_extension_counts_match_the_published_headline` asserts 100 formats / 120 extensions. Change it only together with that test and the copy it lists.
