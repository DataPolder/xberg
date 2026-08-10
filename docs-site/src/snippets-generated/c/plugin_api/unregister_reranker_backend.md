---
id: fixture_c_unregister_reranker_backend
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

unregister_reranker_backend

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* result = ("test-reranker-backend");
    xberg__free(result);
    return EXIT_SUCCESS;
}

```
