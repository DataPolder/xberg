---
id: fixture_c_extract_batch_bytes_invalid_mime
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract_batch with invalid bytes MIME type

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"bytes\":[72,101,108,108,111],\"kind\":\"bytes\",\"mime_type\":\"application/x-nonexistent\"}]");
    XBERGExtractBatch* result = extract_batch(options_handle, NULL);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
