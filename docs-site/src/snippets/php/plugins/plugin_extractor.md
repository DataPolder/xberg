```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\DocumentExtractor;
use Xberg\ExtractInput;
use Xberg\ExtractionConfig;
use Xberg\ExtractedDocument;

class CustomXmlExtractor implements DocumentExtractor {
    public function name(): string {
        return "custom-xml-extractor";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Initialize XML parser resources
    }

    public function shutdown(): void {
        // Cleanup resources
    }

    public function extract(ExtractInput $input, ExtractionConfig $config): ExtractedDocument {
        $content = $input->getBytes() ?? file_get_contents((string) $input->getUri());
        try {
            $xml = simplexml_load_string($content);
            $text = $this->extractTextFromXml($xml);

            return new ExtractedDocument($text, 'application/xml');
        } catch (Exception $e) {
            throw new Exception("XML parsing failed: " . $e->getMessage());
        }
    }

    public function supported_mime_types(): mixed {
        return [
            "application/xml",
            "text/xml",
            "application/xhtml+xml"
        ];
    }

    public function priority(): int {
        return 75;
    }

    private function extractTextFromXml($xml): string {
        $text = "";

        // Extract text from all elements
        foreach ($xml->children() as $child) {
            $childText = (string)$child;
            if (!empty(trim($childText))) {
                $text .= trim($childText) . "\n";
            }
        }

        return $text ?: (string)$xml;
    }
}

// Register the XML extractor
$extractor = new CustomXmlExtractor();
Xberg::registerDocumentExtractor($extractor);

echo "XML extractor registered\n";
```
