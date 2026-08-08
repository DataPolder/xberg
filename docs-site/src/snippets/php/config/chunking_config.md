```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

// Basic chunking
$config = ExtractionConfig::from_json(json_encode([
    'chunking' => [
        'maxCharacters' => 1000,
        'overlap' => 200,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Number of chunks: " . count($result->chunks) . "\n";
foreach ($result->chunks as $chunk) {
    echo "Chunk size: " . strlen($chunk->content) . " characters\n";
}
?>
```

```php title="PHP - Markdown with Heading Context"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'chunking' => [
        'maxCharacters' => 500,
        'overlap' => 50,
        'chunkerType' => 'markdown',
        'prependHeadingContext' => true,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.md'), $config);

$result = $resultOutput->getResults()[0];

foreach ($result->chunks as $chunk) {
    $metadata = $chunk->metadata;
    if ($metadata && $metadata->getHeadingContext()) {
        $headings = $metadata->getHeadingContext()->getHeadings();
        foreach ($headings as $heading) {
            echo "Heading L" . $heading->getLevel() . ": " . $heading->getText() . "\n";
        }
    }
    echo "Content: " . substr($chunk->content, 0, 100) . "...\n";
}
?>
```
