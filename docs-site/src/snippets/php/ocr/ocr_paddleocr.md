```php title="PHP"
<?php
declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\XbergApi;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'ocr' => [
        'backend' => 'paddle-ocr',
        'language' => ['en'],
        // 'paddleOcrConfig' => ['modelTier' => 'server'], // for max accuracy
    ],
]));

$output = \Xberg\XbergApi::extract(\Xberg\ExtractInput::fromUri('scanned_document.pdf'), $config ?? \Xberg\ExtractionConfig::default());
$result = $output->getResults()[0];

echo $result->content . "\n";
```
