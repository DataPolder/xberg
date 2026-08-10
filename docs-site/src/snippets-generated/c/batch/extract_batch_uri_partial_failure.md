---
id: fixture_c_extract_batch_uri_partial_failure
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract_batch with mixed valid and missing URI inputs

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"kind\":\"uri\",\"uri\":\"text/plain.txt\"},{\"kind\":\"uri\",\"uri\":\"/nonexistent/missing.pdf\"}]");
    XBERGExtractBatch* result = extract_batch(options_handle, NULL);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
