# PHP Plugin System

The PHP extension exposes the full plugin registry. Plugins are plain PHP classes
implementing the matching interface; you register them on the `Xberg` facade and
the native extension calls back into PHP during extraction.

## Supported Plugin Types

| Plugin type         | Register                        | Unregister                        | List                          | Clear                          |
| ------------------- | ------------------------------- | --------------------------------- | ----------------------------- | ------------------------------ |
| Document extractor  | `registerDocumentExtractor()`   | `unregisterDocumentExtractor()`   | `listDocumentExtractors()`    | `clearDocumentExtractors()`    |
| OCR backend         | `registerOcrBackend()`          | `unregisterOcrBackend()`          | `listOcrBackends()`           | `clearOcrBackends()`           |
| Post-processor      | `registerPostProcessor()`       | `unregisterPostProcessor()`       | `listPostProcessors()`        | `clearPostProcessors()`        |
| Validator           | `registerValidator()`           | `unregisterValidator()`           | `listValidators()`            | `clearValidators()`            |
| Embedding backend   | `registerEmbeddingBackend()`    | `unregisterEmbeddingBackend()`    | `listEmbeddingBackends()`     | `clearEmbeddingBackends()`     |
| Reranker backend    | `registerRerankerBackend()`     | `unregisterRerankerBackend()`     | `listRerankerBackends()`      | `clearRerankerBackends()`      |
| Tokenizer backend   | `registerTokenizerBackend()`    | `unregisterTokenizerBackend()`    | `listTokenizerBackends()`     | `clearTokenizerBackends()`     |
| Renderer            | `registerRenderer()`            | `unregisterRenderer()`            | `listRenderers()`             | `clearRenderers()`             |

## Quick Start

Register a post-processor that annotates every result with a word count:

```php title="Register a Post-Processor"
<?php

declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

final class WordCountProcessor implements PostProcessor
{
    public function name(): string
    {
        return 'word-count';
    }

    public function version(): string
    {
        return '1.0.0';
    }

    public function initialize(): void
    {
    }

    public function shutdown(): void
    {
    }

    // NOTE: ExtractedDocument's properties are readonly and "metadata" has no
    // writable free-form bag in the current binding — a post-processor cannot
    // attach arbitrary data (like a word count) back onto $result. Flagged
    // rather than guessed at.
    public function process(ExtractedDocument $result, ExtractionConfig $config): mixed
    {
        return null;
    }

    public function processing_stage(): string
    {
        return 'Early';
    }

    public function should_process(ExtractedDocument $result, ExtractionConfig $config): bool
    {
        return $result->content !== '';
    }

    public function estimated_duration_ms(ExtractedDocument $result): int
    {
        return 1;
    }

    public function priority(): int
    {
        return 50;
    }
}

Xberg::registerPostProcessor(new WordCountProcessor());

try {
    $output = Xberg::extract(
        \Xberg\ExtractInput::fromUri('document.pdf'),
        \Xberg\ExtractionConfig::default(),
    );
    echo 'extracted ', strlen($output->getResults()[0]->content), " characters\n";
} catch (\Xberg\XbergException $e) {
    fwrite(STDERR, 'extraction failed: ' . $e->getMessage() . "\n");
} finally {
    Xberg::unregisterPostProcessor('word-count');
}
```

## Plugin Contract

Every plugin may implement four lifecycle/metadata methods — `name()`, `version()`,
`initialize()`, `shutdown()` (plus `description()`/`author()`) — the bridge calls
them if defined, but the interfaces below don't require them. Interface method
names are `snake_case`, matching the underlying Rust trait, not PHP's usual
camelCase convention:

- **DocumentExtractor**: `extract()`, `supported_mime_types()`, plus optional `priority()`, `can_handle()`
- **OcrBackend**: `process_image()`, `supports_language()`, `backend_type()`, plus optional `process_image_file()`, `supported_languages()`, `supports_table_detection()`, `supports_document_processing()`, `emits_structured_markdown()`, `process_document()`
- **PostProcessor**: `process()`, `processing_stage()`, plus optional `should_process()`, `estimated_duration_ms()`, `priority()`
- **Validator**: `validate()`, plus optional `should_validate()`, `priority()`
- **EmbeddingBackend**: `dimensions()`, `embed()`
- **RerankerBackend**: `rerank()`
- **TokenizerBackend**: `count_tokens()`

Priorities run 0-255 and default to 50; the highest-priority plugin matching a
MIME type wins. `initialize()` must validate everything the plugin needs and
throw on failure — registration fails fast rather than erroring mid-extraction.

Never let an exception escape a plugin method during extraction: it crosses the
FFI boundary and aborts the whole extraction. Catch and handle inside the plugin,
or return a degraded result.

## More Examples

- `extractor_registration.md` — custom document extractor
- `ocr_backend.md` — custom OCR backend
- `word_count_processor.md`, `pdf_only_processor.md` — post-processors
- `min_length_validator.md`, `quality_score_validator.md` — validators
- `list_plugins.md`, `unregister_plugins.md`, `clear_plugins.md` — registry management
- `plugin_testing.md` — testing plugins with PHPUnit

## Questions?

- GitHub Issues: <https://github.com/xberg-io/xberg/issues>
- Discussions: <https://github.com/xberg-io/xberg/discussions>
