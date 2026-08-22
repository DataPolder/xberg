---
priority: medium
---

- API stability: plugin interfaces are versioned, breaking changes require major version bump
- Plugin discovery: support both static (compile-time) and dynamic (runtime) registration
- Plugin validation: check capabilities, supported formats, and version compatibility before registration
- Plugin chaining: post-processors can be composed in sequence
- Configuration: plugins accept typed configuration, validated at registration time
- Documentation: every plugin type should have a development guide with examples. Coverage is uneven across the eight types — renderer, tokenizer, and reranker have generated contract snippets only.
