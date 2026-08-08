```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

// Configure chunking with embedding generation for vector database
$config = ExtractionConfig::from_json(json_encode([
    'chunking' => [
        'maxCharacters' => 512,
        'overlap' => 50,
        'chunkerType' => 'semantic',
        'embedding' => [
            'normalize' => true,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri("document.pdf"), $config);

$result = $resultOutput->getResults()[0];

// Store chunks and embeddings for vector database
if ($result->chunks !== null) {
    foreach ($result->chunks as $chunk) {
        // Store in vector database with embedding
        $vectorRecord = [
            "text" => $chunk->text,
            "embedding" => $chunk->embedding ?? [],
            "metadata" => [
                "source" => "document.pdf",
                "page" => $chunk->page_number ?? null,
                "chunk_id" => $chunk->chunk_id ?? null,
            ]
        ];

        // Insert into vector DB (e.g., Pinecone, Weaviate, Milvus)
        // storeInVectorDB($vectorRecord);

        echo "Chunk: " . substr($chunk->text, 0, 50) . "...\n";
        echo "Embedding dimensions: " . count($chunk->embedding ?? []) . "\n";
    }
}
?>
```
