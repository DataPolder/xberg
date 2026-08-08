```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'pages' => [
        'extractPages' => true,
        'insertPageMarkers' => false,
        'markerFormat' => "\n\n<!-- PAGE {page_num} -->\n\n",
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri("document.pdf"), $config);

$result = $resultOutput->getResults()[0];

if ($result->pages !== null) {
    foreach ($result->pages as $page) {
        echo "Page " . $page->page_number . ":\n";
        echo "  Content: " . strlen($page->content) . " chars\n";
        echo "  Tables: " . count($page->tables ?? []) . "\n";
        echo "  Images: " . count($page->images ?? []) . "\n";
    }
}
?>
```
