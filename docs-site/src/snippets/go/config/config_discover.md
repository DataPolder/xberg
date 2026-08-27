```go title="Go"
package main

import (
	"encoding/json"
	"log"
	"os"

	"github.com/xberg-io/xberg/packages/go"
)

func main() {
	configJSON, err := os.ReadFile("xberg.json")
	if err != nil {
		log.Fatalf("read config: %v", err)
	}
	var config xberg.ExtractionConfig
	if err := json.Unmarshal(configJSON, &config); err != nil {
		log.Fatalf("parse config: %v", err)
	}

	input := xberg.ExtractInputFromURI("document.pdf")
	result, err := xberg.Extract(*input, config)
	if err != nil {
		log.Fatalf("extract failed: %v", err)
	}

	log.Printf("Content length: %d", len(result.Results[0].Content))
}
```
