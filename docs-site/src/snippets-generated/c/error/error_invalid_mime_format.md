---
id: fixture_c_error_invalid_mime_format
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

Error when extracting with invalid MIME type format

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    if (result != NULL) { return EXIT_FAILURE; }
    return EXIT_SUCCESS;
}

```
