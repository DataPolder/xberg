```php title="PHP"
<?php
declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\XbergApi;
use Xberg\ExtractionConfig;

// Extract images from documents alongside text
$config = ExtractionConfig::from_json(json_encode([
    'images' => [
        'extractImages' => true,
        'includeDataBase64' => false, // Save images to disk
        'maxImagesPerPage' => 10,
    ],
]));

$output = \Xberg\XbergApi::extract(\Xberg\ExtractInput::fromUri('document_with_images.pdf'), $config ?? \Xberg\ExtractionConfig::default());
$result = $output->getResults()[0];

echo "Extracted Content:\n";
echo $result->content . "\n\n";

if (!empty($result->images)) {
    echo "Extracted " . count($result->images) . " images\n";
    foreach ($result->images as $index => $image) {
        echo "Image " . ($index + 1) . ":\n";
        echo "  Type: " . $image->mimeType . "\n";
        echo "  Size: " . strlen($image->data) . " bytes\n";
        if (isset($image->width) && isset($image->height)) {
            echo "  Dimensions: " . $image->width . "x" . $image->height . "\n";
        }
        echo "\n";
    }
}
?>
```
