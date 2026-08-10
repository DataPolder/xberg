---
id: fixture_php_tokenizer_backends_clear
language: php
target: php
level: typecheck
requires: []
side_effect: safe
---

Clear all tokenizer backends and verify list is empty

```php title="PHP"
<?php

use Xberg\Xberg;
Xberg::clearTokenizerBackends();

```
