---
id: fixture_php_smoke_json_basic
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Smoke test: JSON file extraction

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "mimeType" => "application/json", "uri" => "https://example.com/json/simple.json"]));
$result = Xberg::extract($input, []);

```
