```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

// Configure multilingual language detection
$config = ExtractionConfig::from_json(json_encode([
    'languageDetection' => [
        'enabled' => true,
        'minConfidence' => 0.6,
        'detectMultiple' => true,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri("multilingual_document.pdf"), $config);

$result = $resultOutput->getResults()[0];

// Iterate through all detected languages
if (!empty($result->languages)) {
    echo "Detected " . count($result->languages) . " language(s):\n";

    foreach ($result->languages as $lang) {
        echo "Language: " . $lang->code . "\n";
        if ($lang->confidence !== null) {
            printf("  Confidence: %.1f%%\n", $lang->confidence * 100);
        }
        if ($lang->name !== null) {
            echo "  Name: " . $lang->name . "\n";
        }
    }
} else {
    echo "No languages detected\n";
}
?>
```
