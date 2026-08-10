---
id: fixture_c_config_pages
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests page extraction and page marker configuration

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"pages\":{\"extract_pages\":true,\"insert_page_markers\":true}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
