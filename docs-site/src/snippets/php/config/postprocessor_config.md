```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'postprocessor' => [
        'enabled' => true,
        'enabledProcessors' => [
            'whitespace_normalizer',
            'unicode_normalizer',
        ],
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Processed content: " . substr($result->content, 0, 100) . "...\n";
?>
```
