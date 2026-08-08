```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'pdfOptions' => [
        'hierarchy' => [
            'enabled' => true,
            'kClusters' => 6,
            'includeBbox' => true,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Hierarchy levels: " . count($result->getHierarchy()) . "\n";
?>
```
