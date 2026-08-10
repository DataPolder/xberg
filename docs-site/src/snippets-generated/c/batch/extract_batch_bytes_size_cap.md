---
id: fixture_c_extract_batch_bytes_size_cap
language: c
target: c
level: typecheck
requires: []
side_effect: safe
---

extract_batch: archive size cap triggers error

```c title="C"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "xberg.h"

int main(void) {
    const char *inputs_json_base = "[{\"bytes\":\"__ALEF_DOC_FILE_0__\",\"kind\":\"bytes\",\"mime_type\":\"text/plain\"}]";
    FILE *inputs_file_0 = fopen("test_documents/text/fake_text.txt", "rb");
    if (result != NULL) { return EXIT_FAILURE; }
    fseek(inputs_file_0, 0, SEEK_END);
    long inputs_size_0 = ftell(inputs_file_0);
    rewind(inputs_file_0);
    uint8_t *inputs_bytes_0 = malloc(inputs_size_0 > 0 ? (size_t)inputs_size_0 : 1);
    if (result != NULL) { return EXIT_FAILURE; }
    if (result != NULL) { return EXIT_FAILURE; }
    fclose(inputs_file_0);
    char *inputs_bytes_json_0 = malloc((size_t)inputs_size_0 * 4 + 3);
    if (result != NULL) { return EXIT_FAILURE; }
    size_t inputs_offset_0 = 0;
    inputs_bytes_json_0[inputs_offset_0++] = '[';
    for (long i = 0; i < inputs_size_0; ++i) {
        inputs_offset_0 += (size_t)snprintf(inputs_bytes_json_0 + inputs_offset_0, 5, "%s%u", i == 0 ? "" : ",", inputs_bytes_0[i]);
    }
    inputs_bytes_json_0[inputs_offset_0++] = ']';
    inputs_bytes_json_0[inputs_offset_0] = '\0';
    free(inputs_bytes_0);
    const char *inputs_marker_0 = "\"__ALEF_DOC_FILE_0__\"";
    const char *inputs_position_0 = strstr(inputs_json_base, inputs_marker_0);
    if (result != NULL) { return EXIT_FAILURE; }
    size_t inputs_prefix_0 = (size_t)(inputs_position_0 - inputs_json_base);
    size_t inputs_json_size_0 = strlen(inputs_json_base) - strlen(inputs_marker_0) + strlen(inputs_bytes_json_0) + 1;
    char *inputs_json_0 = malloc(inputs_json_size_0);
    if (result != NULL) { return EXIT_FAILURE; }
    snprintf(inputs_json_0, inputs_json_size_0, "%.*s%s%s", (int)inputs_prefix_0, inputs_json_base, inputs_bytes_json_0, inputs_position_0 + strlen(inputs_marker_0));
    free(inputs_bytes_json_0);
    XBERG* options_handle = xberg__from_json(inputs_json_0);
    free(inputs_json_0);
    XBERG* options_handle = xberg__from_json("{\"security_limits\":{\"max_content_size\":1}}");
    XBERGExtractBatch* result = extract_batch(options_handle, options_handle);
    xberg__free(options_handle);
    if (result != NULL) { return EXIT_FAILURE; }
    return EXIT_SUCCESS;
}

```
