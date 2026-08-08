```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

class LoggingPostProcessor implements PostProcessor {
    public function name(): string {
        return "logging-processor";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        error_log("LoggingPostProcessor initialized");
    }

    public function shutdown(): void {
        error_log("LoggingPostProcessor shutting down");
    }

    public function process(ExtractedDocument $result, ExtractionConfig $config): mixed {
        error_log("Processing: " . $result->mimeType);
        error_log("Content length: " . strlen($result->content));
        error_log("Metadata: " . json_encode($result->getMetadata()));

        return null;
    }

    public function processing_stage(): string {
        return "Early";
    }

    public function should_process(ExtractedDocument $result, ExtractionConfig $config): bool {
        // Only log non-empty results
        return !empty($result->content);
    }

    public function estimated_duration_ms(ExtractedDocument $result): int {
        // Logging takes minimal time
        return 1;
    }

    public function priority(): int {
        return 10;
    }
}

// Register the logging post-processor
$processor = new LoggingPostProcessor();
Xberg::registerPostProcessor($processor);

error_log("Logging post-processor registered");
```
