---
id: fixture_php_renderers_clear
language: php
target: php
level: typecheck
requires: []
side_effect: safe
---

Clear all renderers and verify list is empty

```php title="PHP"
<?php

use Xberg\Xberg;
Xberg::clearRenderers();

```
