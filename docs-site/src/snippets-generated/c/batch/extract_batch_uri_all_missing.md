---
id: fixture_c_extract_batch_uri_all_missing
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract_batch with missing URI inputs

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"kind\":\"uri\",\"uri\":\"/nonexistent/a.pdf\"},{\"kind\":\"uri\",\"uri\":\"/nonexistent/b.txt\"}]");
    XBERGExtractBatch* result = extract_batch(options_handle, NULL);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
