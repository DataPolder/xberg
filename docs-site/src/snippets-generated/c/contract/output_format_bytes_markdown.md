---
id: fixture_c_output_format_bytes_markdown
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

Tests markdown output format via bytes extraction API

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"output_format\":\"markdown\"}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
