---
id: fixture_c_config_chunking_prepend_heading_context
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests markdown chunker records heading hierarchy on chunk metadata

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"chunking\":{\"chunker_type\":\"markdown\",\"max_characters\":500,\"overlap\":50,\"prepend_heading_context\":true}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
