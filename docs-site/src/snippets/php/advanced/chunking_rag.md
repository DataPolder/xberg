```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'chunking' => [
        'maxCharacters' => 500,
        'overlap' => 50,
        'embedding' => [
            'normalize' => true,
            'batchSize' => 32,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('research_paper.pdf'), $config);

$result = $resultOutput->getResults()[0];

if ($result->chunks) {
    foreach ($result->chunks as $chunk) {
        $metadata = $chunk->metadata;
        if ($metadata) {
            echo "Chunk " . ($metadata->getChunkIndex() + 1) . "/" . $metadata->getTotalChunks() . "\n";
            echo "Position: " . $metadata->getByteStart() . "-" . $metadata->getByteEnd() . "\n";
            echo "Content: " . substr($chunk->content, 0, 100) . "...\n";

            if ($chunk->getEmbedding()) {
                echo "Embedding: " . count($chunk->getEmbedding()) . " dimensions\n";
            }
        }
        echo "\n";
    }
}
?>
```
