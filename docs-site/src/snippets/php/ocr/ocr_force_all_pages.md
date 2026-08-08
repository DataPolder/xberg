```php title="PHP"
<?php
declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\XbergApi;
use Xberg\ExtractionConfig;

// Force OCR on all pages, even those with native text
// Useful when native text extraction is unreliable or corrupted
$config = ExtractionConfig::from_json(json_encode([
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng'],
    ],
    // Force OCR on all pages instead of falling back to native text
    'forceOcr' => true,
]));

$output = \Xberg\XbergApi::extract(\Xberg\ExtractInput::fromUri('mixed_scanned_document.pdf'), $config ?? \Xberg\ExtractionConfig::default());
$result = $output->getResults()[0];

echo "Force OCR Results:\n";
echo "All pages processed with OCR\n";
echo "Characters extracted: " . strlen($result->content) . "\n";
echo "Content preview:\n";
echo substr($result->content, 0, 500) . "...\n";

// Without force OCR - uses native text when available
$nativeConfig = ExtractionConfig::from_json(json_encode([
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng'],
    ],
    'forceOcr' => false, // Default: use native text extraction when available
]));

$resultNative = \Xberg\XbergApi::extract(\Xberg\ExtractInput::fromUri('mixed_scanned_document.pdf'), $nativeConfig)->getResults()[0];

echo "\nNative Text Extraction (no force):\n";
echo "Characters extracted: " . strlen($resultNative->content) . "\n";
?>
```
