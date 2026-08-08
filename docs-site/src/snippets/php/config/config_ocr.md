```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng'],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('scanned.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Content length: " . strlen($result->content) . " characters\n";
echo "Tables detected: " . count($result->tables) . "\n";
?>
```
