```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\OcrBackend;
use Xberg\OcrConfig;
use Xberg\ExtractedDocument;

class CustomOcrBackend implements OcrBackend {
    private array $supportedLangs = ["eng", "deu", "fra"];

    public function name(): string {
        return "custom-ocr";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Load OCR model or initialize resources
    }

    public function shutdown(): void {
        // Cleanup OCR resources
    }

    public function process_image(mixed $image_bytes, OcrConfig $config): ExtractedDocument {
        // Process image bytes and return an ExtractedDocument.
        // This would call your OCR engine (Tesseract, PaddleOCR, VLM OCR, etc.)
        return new ExtractedDocument('Extracted text from image', 'image/png', detectedLanguages: ['eng']);
    }

    public function process_image_file(string $path, OcrConfig $config): ExtractedDocument {
        // Read file and delegate to process_image
        $imageBytes = file_get_contents($path);
        return $this->process_image($imageBytes, $config);
    }

    public function supports_language(string $lang): bool {
        return in_array($lang, $this->supportedLangs);
    }

    public function backend_type(): mixed {
        return "OCREngine";
    }

    public function supported_languages(): mixed {
        return $this->supportedLangs;
    }

    public function supports_table_detection(): bool {
        return true;
    }

    public function supports_document_processing(): bool {
        return false;
    }

    public function process_document(string $path, OcrConfig $config): ExtractedDocument {
        throw new Exception("Document processing not supported");
    }
}

// Register the custom OCR backend
$backend = new CustomOcrBackend();
Xberg::registerOcrBackend($backend);

echo "Custom OCR backend registered\n";
```
