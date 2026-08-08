```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'pages' => [
        'extractPages' => true,
        'insertPageMarkers' => true,
        'markerFormat' => "\n\n=== PAGE {page_num} ===\n\n",
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri("document.pdf"), $config);

$result = $resultOutput->getResults()[0];

// Content with inline page markers
echo "Full content with markers:\n";
echo $result->content . "\n\n";

// Or access pages separately with boundaries preserved
if ($result->pages !== null) {
    foreach ($result->pages as $page) {
        echo "--- Page " . $page->page_number . " (boundary) ---\n";
        echo $page->content . "\n";
    }
}
?>
```
