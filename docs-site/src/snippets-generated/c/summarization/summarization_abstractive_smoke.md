---
id: fixture_c_summarization_abstractive_smoke
language: c
target: c
level: typecheck
requires: []
side_effect: server
---

LLM-driven abstractive summary. Skipped automatically when XBERG_LLM_API_KEY (or OPENAI_API_KEY) is not set.

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    XBERG* options_handle = xberg__from_json("{\"summarization\":{\"llm\":{\"max_tokens\":200,\"model\":\"openai/gpt-4o-mini\",\"temperature\":0.0},\"max_tokens\":150,\"strategy\":\"abstractive\"}}");
    XBERGExtract* result = extract(options_handle);
    xberg__free(options_handle);
    xberg_extract_free(result);
    return EXIT_SUCCESS;
}

```
