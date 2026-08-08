```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

class VectorRecord {
    public function __construct(
        public string $id,
        public string $content,
        public array $embedding,
        public array $metadata
    ) {}
}

function extractAndVectorize(
    string $documentPath,
    string $documentId
): array {
    $config = ExtractionConfig::from_json(json_encode([
        'chunking' => [
            'maxCharacters' => 512,
            'overlap' => 50,
            'embedding' => [
                'normalize' => true,
                'batchSize' => 32,
            ],
        ],
    ]));

    $resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri($documentPath), $config);

    $result = $resultOutput->getResults()[0];

    $records = [];
    if ($result->chunks) {
        foreach ($result->chunks as $index => $chunk) {
            $embedding = $chunk->getEmbedding();
            if ($embedding) {
                $metadata = [
                    'document_id' => $documentId,
                    'chunk_index' => (string)$index,
                    'content_length' => (string)strlen($chunk->content),
                ];

                $records[] = new VectorRecord(
                    id: "{$documentId}_chunk_{$index}",
                    content: $chunk->content,
                    embedding: $embedding,
                    metadata: $metadata
                );
            }
        }
    }

    return $records;
}

// Usage
$records = extractAndVectorize('research_paper.pdf', 'doc_123');

foreach ($records as $record) {
    echo "Vector ID: " . $record->id . "\n";
    echo "Content length: " . strlen($record->content) . " characters\n";
    echo "Embedding dimension: " . count($record->embedding) . "\n";
    echo "---\n";
}
?>
```
