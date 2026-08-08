```php
<?php
use Xberg\XbergApi;
use Xberg\EmbeddingConfig;

// Embed with default config (balanced preset)
$embeddings = $xberg->embed(["Hello world", "How are you?"]);

// Embed with specific preset
$config = EmbeddingConfig::from_json(json_encode([
    'model' => ['type' => 'preset', 'name' => 'fast'],
]));
$embeddings = $xberg->embed(["Hello world"], $config);

// Each embedding is a float array
foreach ($embeddings as $i => $vector) {
    echo "Text $i: " . count($vector) . " dimensions\n";
}
```
