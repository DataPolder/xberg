```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng', 'deu'],
        'tesseractConfig' => [
            'psm' => 6,
            'oem' => 3,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('scanned.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "OCR text: " . substr($result->content, 0, 100) . "...\n";
?>
```
