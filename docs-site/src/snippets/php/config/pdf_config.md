```php title="PHP"
<?php

declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

/**
 * PDF configuration with hierarchy detection
 */
$config = ExtractionConfig::from_json(json_encode([
    'pdfOptions' => [
        'extractImages' => true,
        'extractMetadata' => true,
        'passwords' => ['password1', 'password2'],
        'hierarchy' => [
            'enabled' => true,
            'kClusters' => 6,
            'includeBbox' => true,
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);
$result = $resultOutput->getResults()[0];

echo "Content length: " . strlen($result->content) . " characters\n";
echo "Metadata: " . implode(', ', array_keys((array) ($result->metadata?->pdf ?? []))) . "\n";
```
