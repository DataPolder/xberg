```go title="Go"
package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"strings"

	"github.com/xberg-io/xberg/packages/go"
)

// jsonExtractor implements xberg.DocumentExtractor and flattens every string
// leaf of a JSON document into plain text.
type jsonExtractor struct{}

func (jsonExtractor) Name() string      { return "custom-json-extractor" }
func (jsonExtractor) Version() string   { return "1.0.0" }
func (jsonExtractor) Initialize() error { return nil }
func (jsonExtractor) Shutdown() error   { return nil }

func (jsonExtractor) SupportedMimeTypes() []string {
	return []string{"application/json", "text/json"}
}

// Priority 60 outranks the built-in extractors (50) for the MIME types above.
func (jsonExtractor) Priority() int32 { return 60 }

func (jsonExtractor) CanHandle(_path string, _mimeType string) bool { return true }

func (jsonExtractor) Extract(
	input xberg.ExtractInput,
	_config xberg.ExtractionConfig,
) (xberg.ExtractedDocument, error) {
	// `kind = "bytes"` inputs carry the payload directly; `kind = "uri"` inputs
	// carry a path the plugin reads itself.
	payload := input.Bytes
	if payload == nil {
		if input.URI == nil {
			return xberg.ExtractedDocument{}, fmt.Errorf("custom-json-extractor: input has neither bytes nor uri")
		}
		read, err := os.ReadFile(*input.URI)
		if err != nil {
			return xberg.ExtractedDocument{}, fmt.Errorf("custom-json-extractor: reading %s: %w", *input.URI, err)
		}
		payload = read
	}

	var decoded any
	if err := json.Unmarshal(payload, &decoded); err != nil {
		return xberg.ExtractedDocument{}, fmt.Errorf("custom-json-extractor: parsing JSON: %w", err)
	}

	var text strings.Builder
	flatten(decoded, &text)

	return xberg.ExtractedDocument{
		Content:  text.String(),
		MimeType: "application/json",
	}, nil
}

func flatten(value any, out *strings.Builder) {
	switch typed := value.(type) {
	case string:
		out.WriteString(typed)
		out.WriteString("\n")
	case []any:
		for _, item := range typed {
			flatten(item, out)
		}
	case map[string]any:
		for _, item := range typed {
			flatten(item, out)
		}
	}
}

func main() {
	if err := xberg.RegisterDocumentExtractor(jsonExtractor{}); err != nil {
		log.Fatalf("register extractor failed: %v", err)
	}
	defer func() {
		if err := xberg.UnregisterDocumentExtractor("custom-json-extractor"); err != nil {
			log.Printf("warning: unregister failed: %v", err)
		}
	}()

	input := xberg.ExtractInputFromURI("document.json")
	result, err := xberg.Extract(*input, xberg.ExtractionConfig{})
	if err != nil {
		log.Fatalf("extract failed: %v", err)
	}

	log.Printf("extracted content length: %d", len(result.Results[0].Content))
}
```
