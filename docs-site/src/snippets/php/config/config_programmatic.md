```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\ExtractionConfig;

$config = ExtractionConfig::from_json(json_encode([
    'useCache' => true,
    'ocr' => [
        'backend' => 'tesseract',
        'language' => ['eng', 'deu'],
        'tesseractConfig' => ['psm' => 6],
    ],
    'chunking' => [
        'maxCharacters' => 1000,
        'overlap' => 200,
    ],
    'enableQualityProcessing' => true,
]));

$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);

$result = $resultOutput->getResults()[0];

echo "Content length: " . strlen($result->content) . " characters\n";
?>
```
