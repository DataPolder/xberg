---
id: fixture_php_error_unsupported_mime
language: php
target: php
level: typecheck
requires: []
side_effect: safe
---

Error when extracting with unsupported MIME type

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
$input = \Xberg\ExtractInput::from_json(json_encode(["bytes" => "test_documents/text/plain.txt", "config" => [], "filename" => "plain.txt", "kind" => "bytes", "mimeType" => "application/x-nonexistent"]));
try {
    Xberg::extract($input, []);
} catch (Throwable $error) {
    echo "Call failed as expected: {$error->getMessage()}\n";
    return;
}
throw new RuntimeException('expected call to fail');

```
