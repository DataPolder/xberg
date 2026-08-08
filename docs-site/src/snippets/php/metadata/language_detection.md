```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

// Configure language detection with confidence threshold
$config = ExtractionConfig::from_json(json_encode([
    'languageDetection' => [
        'enabled' => true,
        'minConfidence' => 0.7,
        'detectMultiple' => false,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri("document.pdf"), $config);

$result = $resultOutput->getResults()[0];

// Access detected languages
if (!empty($result->languages)) {
    foreach ($result->languages as $lang) {
        echo "Detected language: " . $lang->code . "\n";
        if ($lang->confidence !== null) {
            echo "Confidence: " . $lang->confidence . "\n";
        }
    }
}
?>
```
