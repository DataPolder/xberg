---
id: fixture_c_url_batch_mixed_inputs
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

extract_batch: mixed bytes and URL inputs share one output envelope

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("[{\"kind\":\"uri\",\"uri\":\"https://example.com\"},{\"bytes\":[66,97,116,99,104,32,98,121,116,101,115,32,99,111,110,116,101,110,116],\"filename\":\"inline.txt\",\"kind\":\"bytes\",\"mime_type\":\"text/plain\"}]");
    XBERG* options_handle = xberg__from_json("{\"url\":{\"mode\":\"document\"}}");
    XBERGExtractBatch* result = extract_batch(options_handle, options_handle);
    xberg__free(options_handle);
    xberg_extract_batch_free(result);
    return EXIT_SUCCESS;
}

```
