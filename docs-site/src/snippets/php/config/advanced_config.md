```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

// Advanced configuration combining multiple features
$config = ExtractionConfig::from_json(json_encode([
    'useCache' => true,
    'enableQualityProcessing' => true,
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng'],
    ],
    'chunking' => [
        'maxCharacters' => 1000,
        'overlap' => 200,
    ],
    'languageDetection' => [
        'enabled' => true,
        'minConfidence' => 0.8,
        'detectMultiple' => false,
    ],
    'tokenReduction' => [
        'mode' => 'moderate',
        'preserveImportantWords' => true,
    ],
    'postprocessor' => [
        'enabled' => true,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Content length: " . strlen($result->content) . " characters\n";
if ($result->getDetectedLanguages()) {
    echo "Languages: " . implode(', ', $result->getDetectedLanguages()) . "\n";
}
?>
```
