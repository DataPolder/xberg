```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'chunking' => [
        'maxCharacters' => 1000,
        'overlap' => 200,
        'embedding' => [
            'model' => ['type' => 'preset', 'name' => 'balanced'],
            'batchSize' => 16,
            'normalize' => true,
            'showDownloadProgress' => true,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Chunks with embeddings: " . count($result->chunks) . "\n";
?>
```
