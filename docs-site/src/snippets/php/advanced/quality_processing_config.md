```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = new ExtractionConfig(
    enableQualityProcessing: true,
    useCache: true
);

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

if ($result->getQualityScore() !== null) {
    echo "Quality score: " . $result->getQualityScore() . "\n";
}

if ($result->getProcessingTime() !== null) {
    echo "Processing time: " . $result->getProcessingTime() . "ms\n";
}
?>
```
