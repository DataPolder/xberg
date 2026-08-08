```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

// Build config with OCR, chunking, and image extraction
$config = ExtractionConfig::from_json(json_encode([
    'useCache' => true,
    'forceOcr' => false,
    'outputFormat' => 'markdown',
    'includeDocumentStructure' => true,
    'enableQualityProcessing' => true,
    // OCR: Tesseract with English language
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng'],
    ],
    // Chunking: markdown chunks ~800 chars, 100-char overlap
    'chunking' => [
        'maxCharacters' => 800,
        'overlap' => 100,
        'trim' => true,
        'chunkerType' => 'markdown',
        'prependHeadingContext' => true,
    ],
    // Image extraction
    'images' => [
        'extractImages' => true,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('report.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Content (" . strlen($result->content) . " chars):\n";
echo substr($result->content, 0, 200) . "\n\n";

if ($result->chunks !== null) {
    echo "Chunks: " . count($result->chunks) . "\n";
}
echo "Tables: " . count($result->tables) . "\n";

if ($result->getDetectedLanguages() !== null) {
    echo "Languages: " . implode(', ', $result->getDetectedLanguages()) . "\n";
}

if ($result->getExtractionMethod() !== null) {
    echo "Extraction method: " . $result->getExtractionMethod() . "\n";
}
```
