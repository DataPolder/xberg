---
id: fixture_c_url_html_page_extract
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

extract: website URL returns page content

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"url\":{\"mode\":\"document\"}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
