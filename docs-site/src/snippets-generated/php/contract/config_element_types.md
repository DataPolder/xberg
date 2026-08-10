---
id: fixture_php_config_element_types
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Tests element-based result format with element type assertions on DOCX

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "uri" => "https://example.com/docx/unit_test_headers.docx"]));
$result = Xberg::extract($input, ["result_format" => "element_based"]);

```
