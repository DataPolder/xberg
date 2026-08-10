---
id: fixture_c_extract_batch_uri_basic
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract_batch over URI inputs

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"kind\":\"uri\",\"uri\":\"pdf/fake_memo.pdf\"},{\"kind\":\"uri\",\"uri\":\"text/fake_text.txt\"}]");
    XBERGExtractBatch* result = extract_batch(options_handle, NULL);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
