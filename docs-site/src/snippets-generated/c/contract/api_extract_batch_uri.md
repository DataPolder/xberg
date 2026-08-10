---
id: fixture_c_api_extract_batch_uri
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests batch URI extraction API (extract_batch)

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"kind\":\"uri\",\"uri\":\"https://example.com/pdf/fake_memo.pdf\"}]");
    XBERGExtractBatch* result = extract_batch(options_handle, NULL);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
