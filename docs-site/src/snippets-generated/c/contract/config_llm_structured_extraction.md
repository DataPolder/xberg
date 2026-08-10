---
id: fixture_c_config_llm_structured_extraction
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

Tests structured extraction via liter-llm with JSON schema

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"structured_extraction\":{\"llm\":{\"model\":\"openai/gpt-4o\"},\"schema\":{\"properties\":{\"date\":{\"type\":\"string\"},\"summary\":{\"type\":\"string\"},\"title\":{\"type\":\"string\"}},\"required\":[\"title\"],\"type\":\"object\"},\"schema_name\":\"memo_data\"}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
