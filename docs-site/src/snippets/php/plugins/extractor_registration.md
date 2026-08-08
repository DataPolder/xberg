```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\DocumentExtractor;
use Xberg\ExtractInput;
use Xberg\ExtractionConfig;
use Xberg\ExtractedDocument;

class CustomJsonExtractor implements DocumentExtractor {
    public function name(): string {
        return "custom-json-extractor";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Initialize resources
    }

    public function shutdown(): void {
        // Cleanup resources
    }

    public function extract(ExtractInput $input, ExtractionConfig $config): ExtractedDocument {
        $content = $input->getBytes() ?? file_get_contents((string) $input->getUri());
        $json = json_decode($content, true);
        $text = $this->extractTextFromJson($json);

        return new ExtractedDocument($text, 'application/json');
    }

    public function supported_mime_types(): mixed {
        return ["application/json", "text/json"];
    }

    public function priority(): int {
        return 50;
    }

    private function extractTextFromJson($value): string {
        if (is_string($value)) {
            return "$value\n";
        }
        if (is_array($value)) {
            $result = "";
            foreach ($value as $item) {
                $result .= $this->extractTextFromJson($item);
            }
            return $result;
        }
        return "";
    }
}

// Register the custom extractor
$extractor = new CustomJsonExtractor();
Xberg::registerDocumentExtractor($extractor);
```
