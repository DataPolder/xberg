---
id: fixture_php_smoke_docx_basic
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Smoke test: DOCX with formatted text

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "mimeType" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "uri" => "https://example.com/docx/fake.docx"]));
$result = Xberg::extract($input, []);

```
