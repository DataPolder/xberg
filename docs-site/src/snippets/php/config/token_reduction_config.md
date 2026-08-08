```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'tokenReduction' => [
        'mode' => 'moderate',
        'preserveImportantWords' => true,
    ],
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Original token count: " . $result->getTokenCount() . "\n";
echo "Reduced content: " . substr($result->content, 0, 100) . "...\n";
?>
```
