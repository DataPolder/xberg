---
id: fixture_php_extract_batch_uri_basic
language: php
target: php
level: typecheck
requires: []
side_effect: safe
---

extract_batch over URI inputs

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractInput;
use Xberg\ExtractionConfig;
$result = Xberg::extractBatch([ExtractInput::from_json('{"kind":"uri","uri":"pdf/fake_memo.pdf"}'), ExtractInput::from_json('{"kind":"uri","uri":"text/fake_text.txt"}')], \Xberg\ExtractionConfig::from_json('{}'));

```
