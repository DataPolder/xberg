---
id: fixture_c_url_crawl_linked_pages
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

extract: crawl mode follows linked pages

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"url\":{\"crawl\":{\"max_depth\":1,\"max_pages\":4,\"respect_robots_txt\":false},\"mode\":\"crawl\"}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
