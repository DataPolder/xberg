```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

class PdfOnlyProcessor implements PostProcessor {
    public function name(): string {
        return "pdf-only-processor";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Initialize PDF-specific resources
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
        // Only process PDFs with content
        return $result->mimeType === 'application/pdf' && !empty($result->content);
    }

    public function estimated_duration_ms(ExtractedDocument $result): int {
        // PDF processing varies by size
        return 50;
    }

    public function priority(): int {
        return 75;
    }
}

// Register the PDF-only processor
$processor = new PdfOnlyProcessor();
Xberg::registerPostProcessor($processor);

echo "PDF-only processor registered\n";
```
