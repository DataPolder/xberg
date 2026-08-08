```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'languageDetection' => [
        'enabled' => true,
        'minConfidence' => 0.8,
        'detectMultiple' => true,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('multilingual_document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Detected languages: ";
$languages = $result->getDetectedLanguages();
if ($languages) {
    echo implode(", ", $languages) . "\n";
} else {
    echo "None\n";
}

echo "Primary language: " . $result->getLanguage() . "\n";
echo "Confidence: " . $result->getLanguageConfidence() . "\n";
?>
```
