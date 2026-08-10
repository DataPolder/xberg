---
id: fixture_c_error_extract_input_conflicting_ocr
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract force+disable OCR

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"disable_ocr\":true,\"force_ocr\":true}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    if (result != NULL) { return EXIT_FAILURE; }
    return EXIT_SUCCESS;
}

```
