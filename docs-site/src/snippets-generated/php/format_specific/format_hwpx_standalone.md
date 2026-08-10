---
id: fixture_php_format_hwpx_standalone
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Standalone HWPX extraction using extract

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["filename" => "simple.hwpx", "kind" => "uri", "mimeType" => "application/haansofthwpx", "uri" => "https://example.com/hwpx/simple.hwpx"]));
$result = Xberg::extract($input, null);

```
