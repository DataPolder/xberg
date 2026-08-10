---
id: fixture_c_config_document_structure_with_headings
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests document structure with DOCX heading-driven nesting

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"include_document_structure\":true}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
