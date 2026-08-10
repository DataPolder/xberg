---
id: fixture_php_ocr_backends_unregister
language: php
target: php
level: typecheck
requires: []
side_effect: safe
---

Unregister nonexistent OCR backend gracefully

```php title="PHP"
<?php

use Xberg\Xberg;
Xberg::unregisterOcrBackend("nonexistent-backend-xyz");

```
