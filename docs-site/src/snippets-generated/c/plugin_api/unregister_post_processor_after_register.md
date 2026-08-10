---
id: fixture_c_unregister_post_processor_after_register
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

unregister_post_processor

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* result = ("test-processor");
    xberg__free(result);
    return EXIT_SUCCESS;
}

```
