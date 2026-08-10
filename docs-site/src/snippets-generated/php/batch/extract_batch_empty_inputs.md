---
id: fixture_php_extract_batch_empty_inputs
language: php
target: php
level: typecheck
requires: []
side_effect: safe
---

extract_batch: empty batch

```php title="PHP"
<?php

use Xberg\Xberg;
use Xberg\ExtractionConfig;
$result = Xberg::extractBatch([], \Xberg\ExtractionConfig::from_json('{}'));

```
