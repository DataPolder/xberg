```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

class StatefulPlugin implements PostProcessor {
    private int $callCount = 0;
    private array $cache = [];

    public function name(): string {
        return "stateful-plugin";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        $this->callCount = 0;
        $this->cache = [];
        error_log("StatefulPlugin initialized");
    }

    public function shutdown(): void {
        error_log("StatefulPlugin called {$this->callCount} times total");
    }

    // NOTE: ExtractedDocument's properties are readonly and "metadata" has no
    // writable free-form bag in the current binding — only this plugin's own
    // instance state (cache/callCount below) can actually be tracked; the
    // metadata annotation shown in earlier revisions of this snippet is not
    // supported and has been removed rather than guessed at.
    public function process(ExtractedDocument $result, ExtractionConfig $config): mixed {
        $this->callCount++;

        // Cache the last MIME type
        $this->cache['last_mime'] = $result->mimeType;
        $this->cache['last_timestamp'] = time();

        return null;
    }

    public function processing_stage(): string {
        return "Middle";
    }

    public function should_process(ExtractedDocument $result, ExtractionConfig $config): bool {
        // Always process to track state
        return true;
    }

    public function estimated_duration_ms(ExtractedDocument $result): int {
        // State tracking is minimal overhead
        return 2;
    }

    public function priority(): int {
        return 50;
    }

    public function getCallCount(): int {
        return $this->callCount;
    }

    public function getCache(): array {
        return $this->cache;
    }
}

// Register the stateful plugin
$plugin = new StatefulPlugin();
Xberg::registerPostProcessor($plugin);

echo "Stateful plugin registered\n";
// Can later retrieve state: $plugin->getCallCount(), $plugin->getCache()
```
