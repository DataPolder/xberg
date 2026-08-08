```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

class PdfMetadataExtractor implements PostProcessor {
    public function name(): string {
        return "pdf-metadata-extractor";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Load PDF parsing libraries if needed
    }

    public function shutdown(): void {
        // Cleanup resources
    }

    // NOTE: ExtractedDocument's properties are readonly and "metadata" has no
    // writable free-form bag in the current binding — a post-processor cannot
    // attach arbitrary data to $result the way this example implies. Flagged
    // rather than guessed at.
    public function process(ExtractedDocument $result, ExtractionConfig $config): mixed {
        return null;
    }

    public function processing_stage(): string {
        return "Middle";
    }

    public function should_process(ExtractedDocument $result, ExtractionConfig $config): bool {
        return $result->mimeType === 'application/pdf';
    }

    public function estimated_duration_ms(ExtractedDocument $result): int {
        return 10;
    }

    public function priority(): int {
        return 60;
    }
}

// Register the PDF metadata extractor
$processor = new PdfMetadataExtractor();
Xberg::registerPostProcessor($processor);

echo "PDF metadata extractor registered\n";
```
