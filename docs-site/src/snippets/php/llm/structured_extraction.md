<!-- snippet:syntax-only -->

```php title="PHP"
<?php

declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\XbergApi;
use Xberg\ExtractionConfig;

$schema = [
    'type' => 'object',
    'properties' => [
        'title' => ['type' => 'string'],
        'authors' => ['type' => 'array', 'items' => ['type' => 'string']],
        'date' => ['type' => 'string'],
    ],
    'required' => ['title', 'authors', 'date'],
    'additionalProperties' => false,
];

$config = ExtractionConfig::from_json(json_encode([
    'structuredExtraction' => [
        'schema' => $schema,
        'schemaName' => 'paper_metadata',
        'strict' => true,
        'llm' => [
            'model' => 'openai/gpt-4o-mini',
        ],
    ],
], JSON_THROW_ON_ERROR));

$output = \Xberg\XbergApi::extract(\Xberg\ExtractInput::fromUri('paper.pdf'), $config ?? \Xberg\ExtractionConfig::default());
$result = $output->getResults()[0];

if ($result->structured_output !== null) {
    echo $result->structured_output, "\n";
}
```
