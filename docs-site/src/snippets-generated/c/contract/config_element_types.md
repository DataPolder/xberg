---
id: fixture_c_config_element_types
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests element-based result format with element type assertions on DOCX

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"result_format\":\"element_based\"}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
