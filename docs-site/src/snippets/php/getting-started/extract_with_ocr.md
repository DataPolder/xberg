```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'forceOcr' => true,
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng'],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('scanned.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Content:\n";
echo $result->content;

if ($result->getDetectedLanguages() !== null) {
    echo "Detected Languages: " . implode(', ', $result->getDetectedLanguages()) . "\n";
}
```
