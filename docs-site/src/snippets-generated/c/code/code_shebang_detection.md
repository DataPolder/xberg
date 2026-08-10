---
id: fixture_c_code_shebang_detection
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Test language detection from shebang line via bytes input

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERGExtract* result = extract(NULL);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
