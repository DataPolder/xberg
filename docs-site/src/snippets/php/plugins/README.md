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

    public function process(object &$result, object $config): void
    {
        $words = preg_split('/\s+/', trim($result->content), -1, PREG_SPLIT_NO_EMPTY);
        $metadata = (array) ($result->metadata ?? []);
        $metadata['word_count'] = count($words);
        $result->metadata = $metadata;
    }

    public function processingStage(): string
    {
        return 'Early';
    }

    public function shouldProcess(object $result, object $config): bool
    {
        return $result->content !== '';
    }

    public function estimatedDurationMs(object $result): int
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
    $metadata = (array) $output->results[0]->metadata;
    echo 'word count: ', $metadata['word_count'] ?? 0, "\n";
} catch (\Xberg\Exceptions\XbergException $e) {
    fwrite(STDERR, 'extraction failed: ' . $e->getMessage() . "\n");
} finally {
    Xberg::unregisterPostProcessor('word-count');
}
```

## Plugin Contract

Every plugin implements the four lifecycle methods — `name()`, `version()`,
`initialize()`, `shutdown()` — plus the methods specific to its type:

- **DocumentExtractor**: `extract()`, `supportedMimeTypes()`, `priority()`
- **OcrBackend**: `processImage()`, `supportsLanguage()`, `backendType()`
- **PostProcessor**: `process()`, `processingStage()`, `shouldProcess()`, `estimatedDurationMs()`, `priority()`
- **Validator**: `validate()`, `priority()`

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
