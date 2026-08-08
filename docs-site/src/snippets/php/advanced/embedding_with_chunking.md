```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'chunking' => [
        'maxCharacters' => 1024,
        'overlap' => 100,
        'embedding' => [
            'normalize' => true,
            'batchSize' => 32,
            'showDownloadProgress' => false,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

if ($result->chunks) {
    foreach ($result->chunks as $chunk) {
        echo "Chunk content: " . substr($chunk->content, 0, 100) . "...\n";

        $embedding = $chunk->getEmbedding();
        if ($embedding) {
            echo "Embedding dimension: " . count($embedding) . "\n";
            echo "First 5 values: ";
            echo implode(", ", array_slice($embedding, 0, 5));
            echo "\n";
        }
        echo "\n";
    }
}
?>
```
