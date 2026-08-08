```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'images' => [
        'extractImages' => true,
        'targetDpi' => 300,
        'maxImageDimension' => 4096,
        'autoAdjustDpi' => true,
        'minDpi' => 150,
        'maxDpi' => 600,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Extracted images: " . count($result->getImages()) . "\n";
?>
```
