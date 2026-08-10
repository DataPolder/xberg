---
id: fixture_php_smoke_txt_basic
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Smoke test: Plain text file

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "mimeType" => "text/plain", "uri" => "https://example.com/text/report.txt"]));
$result = Xberg::extract($input, []);

```
