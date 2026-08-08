```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\EmbeddingBackend;
use Xberg\EmbeddingConfig;

class MyEmbedder implements EmbeddingBackend {
    public function name(): string {
        return "my-embedder";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Initialize the embedding model
    }

    public function shutdown(): void {
        // Cleanup resources
    }

    public function dimensions(): int {
        return 768;
    }

    public function embed(mixed $texts): mixed {
        // Delegate to your already-loaded host model
        // Return array of embedding vectors
        $embeddings = [];
        foreach ($texts as $text) {
            $embeddings[] = array_fill(0, 768, 0.0);
        }
        return $embeddings;
    }
}

// Register the embedding backend at startup
$embedder = new MyEmbedder();
Xberg::registerEmbeddingBackend($embedder);

// NOTE: the remainder of this example does not currently work against the
// PHP binding and is left unresolved rather than guessed at:
//   - EmbeddingConfig::model has no ext-php-rs constructor/prop support, so it
//     cannot be set to "my-embedder" (or anything else) from PHP at all —
//     see stubs/xberg_extension.php's EmbeddingConfig, whose constructor only
//     accepts normalize/batchSize/showDownloadProgress/cacheDir/
//     maxEmbedDurationSecs/maxSequenceLength.
//   - Xberg::embedTexts() does not exist anywhere in the binding (Xberg.php /
//     the native XbergApi class expose no standalone embed call); embedding
//     currently only happens as part of a full Xberg::extract() call via
//     ExtractionConfig's (also currently unsettable) embedding config.
$config = EmbeddingConfig::default();
```
