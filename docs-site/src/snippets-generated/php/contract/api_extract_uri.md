---
id: fixture_php_api_extract_uri
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Tests URI extraction API

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "uri" => "https://example.com/pdf/fake_memo.pdf"]));
$result = Xberg::extract($input, null);

```
