---
id: fixture_php_format_pptx
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

PPTX presentation extraction using extract

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "mimeType" => "application/vnd.openxmlformats-officedocument.presentationml.presentation", "uri" => "https://example.com/pptx/simple.pptx"]));
$result = Xberg::extract($input, null);

```
