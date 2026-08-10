---
id: fixture_php_format_xlsx
language: php
target: php
level: typecheck
requires: []
side_effect: server
---

XLSX spreadsheet extraction using extract

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["kind" => "uri", "mimeType" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "uri" => "https://example.com/xlsx/stanley_cups.xlsx"]));
$result = Xberg::extract($input, null);

```
