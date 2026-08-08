```php title="PHP"
<?php
declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;

// Discover configuration from file system
$config = ExtractionConfig::discover() ?? ExtractionConfig::default();
$resultOutput = Xberg::extract(\Xberg\ExtractInput::fromUri('document.pdf'), $config);
$result = $resultOutput->getResults()[0];

echo $result->content;
?>
```
