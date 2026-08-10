---
id: fixture_c_extract_batch_bytes_mixed_format
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract_batch: handles unsupported MIME gracefully

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"bytes\":[80,68,70,32,112,108,97,99,101,104,111,108,100,101,114],\"kind\":\"bytes\",\"mime_type\":\"application/x-unknown\"}]");
    XBERGExtractBatch* result = extract_batch(options_handle, NULL);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
