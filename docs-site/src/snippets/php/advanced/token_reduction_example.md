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

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('verbose_document.pdf'), $config);

$result = $resultOutput->getResults()[0];

if ($result->getTokenCount() !== null) {
    echo "Original token count: " . $result->getTokenCount() . "\n";
}

// Access the reduced content
echo "Reduced content length: " . strlen($result->content) . " characters\n";
echo "Content preview: " . substr($result->content, 0, 100) . "...\n";
?>
```
