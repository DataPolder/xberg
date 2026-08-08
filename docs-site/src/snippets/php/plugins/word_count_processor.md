```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

class WordCountProcessor implements PostProcessor {
    public function name(): string {
        return "word-count";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Initialize word counting resources
    }

    public function shutdown(): void {
        // Cleanup resources
    }

    // NOTE: ExtractedDocument's properties are readonly and "metadata" has no
    // writable free-form bag in the current binding — a post-processor cannot
    // attach arbitrary data to $result the way this example implies. Flagged
    // rather than guessed at; word count is computed but has nowhere to go.
    public function process(ExtractedDocument $result, ExtractionConfig $config): mixed {
        $wordCount = count(preg_split('/\s+/', trim($result->content), -1, PREG_SPLIT_NO_EMPTY));

        return null;
    }

    public function processing_stage(): string {
        return "Early";
    }

    public function should_process(ExtractedDocument $result, ExtractionConfig $config): bool {
        // Only process if content is not empty
        return !empty($result->content);
    }

    public function estimated_duration_ms(ExtractedDocument $result): int {
        // Word counting is very fast
        return 1;
    }

    public function priority(): int {
        return 50;
    }
}

// Register the word-count post-processor
$processor = new WordCountProcessor();
Xberg::registerPostProcessor($processor);

echo "Word-count processor registered\n";
```
