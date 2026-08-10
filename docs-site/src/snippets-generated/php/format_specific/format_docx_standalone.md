---
id: fixture_php_format_docx_standalone
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

Standalone DOCX extraction using extract

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["filename" => "fake.docx", "kind" => "uri", "mimeType" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "uri" => "https://example.com/docx/fake.docx"]));
$result = Xberg::extract($input, null);

```
