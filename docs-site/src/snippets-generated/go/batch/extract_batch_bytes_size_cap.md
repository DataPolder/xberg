---
id: fixture_go_extract_batch_bytes_size_cap
language: go
target: go
level: typecheck
requires: []
side_effect: safe
---

extract_batch: archive size cap triggers error

```go title="Go"
package main

import (
	"encoding/json"
	"fmt"
	xberg "github.com/xberg-io/xberg/packages/go"
)

func main() {
	var inputs []xberg.ExtractInput
	if err := json.Unmarshal([]byte(`[{"bytes":"test_documents/text/fake_text.txt","kind":"bytes","mime_type":"text/plain"}]`), &inputs); err != nil {
		panic(fmt.Sprintf("config parse failed: %v", err))
	}
	config := xberg.ExtractionConfig{
		SecurityLimits: &xberg.SecurityLimits{
		MaxContentSize: 1,
	},
	}
	result, err := xberg.ExtractBatch(inputs, config)
	if err != nil {
		panic(err)
	}
	fmt.Println(result)
}
```
