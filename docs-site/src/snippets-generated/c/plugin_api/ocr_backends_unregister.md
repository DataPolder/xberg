---
id: fixture_c_ocr_backends_unregister
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

Unregister nonexistent OCR backend gracefully

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* result = ("nonexistent-backend-xyz");
    xberg__free(result);
    return EXIT_SUCCESS;
}

```
