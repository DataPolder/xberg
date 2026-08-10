---
id: fixture_php_config_keywords
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Tests keyword extraction via YAKE algorithm

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "uri" => "https://example.com/pdf/fake_memo.pdf"]));
$result = Xberg::extract($input, ["keywords" => ["algorithm" => "yake", "max_keywords" => 10]]);

```
