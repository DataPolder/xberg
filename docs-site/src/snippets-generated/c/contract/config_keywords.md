---
id: fixture_c_config_keywords
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests keyword extraction via YAKE algorithm

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"keywords\":{\"algorithm\":\"yake\",\"max_keywords\":10}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
