```php title="PHP"
<?php
declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\XbergApi;
use Xberg\ExtractionConfig;

// Cloud-based OCR using Vision Language Model (VLM)
// Requires API key and model configuration
$config = ExtractionConfig::from_json(json_encode([
    'ocr' => [
        'backend' => 'vlm',
        'language' => ['eng'],
        'vlmConfig' => [
            'model' => 'anthropic/claude-3-5-sonnet-20241022',
            'apiKey' => getenv('ANTHROPIC_API_KEY'),
        ],
        'vlmPrompt' => 'Extract all text from this document page. Preserve formatting and structure.',
    ],
]));

$output = \Xberg\XbergApi::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config ?? \Xberg\ExtractionConfig::default());
$result = $output->getResults()[0];

echo "Cloud OCR Results:\n";
echo "Content length: " . strlen($result->content) . " characters\n";
echo "Preview: " . substr($result->content, 0, 200) . "...\n";
?>
```
