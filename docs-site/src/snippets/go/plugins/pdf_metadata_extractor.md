```go title="Go"
package main

import (
	"encoding/json"
	"log"
	"sync/atomic"

	"github.com/xberg-io/xberg/packages/go"
)

type pdfMetadataExtractor struct {
	processedCount atomic.Int64
}

func (processor *pdfMetadataExtractor) Name() string       { return "pdf_metadata_extractor" }
func (processor *pdfMetadataExtractor) Version() string    { return "1.0.0" }
func (processor *pdfMetadataExtractor) Initialize() error  { return nil }
func (processor *pdfMetadataExtractor) Shutdown() error    { return nil }
func (processor *pdfMetadataExtractor) Priority() int32    { return 80 }
func (processor *pdfMetadataExtractor) ProcessingStage() xberg.ProcessingStage {
	return xberg.ProcessingStageEarly
}
func (processor *pdfMetadataExtractor) ShouldProcess(
	result xberg.ExtractedDocument,
	_ xberg.ExtractionConfig,
) bool {
	return result.MimeType == "application/pdf"
}
func (processor *pdfMetadataExtractor) EstimatedDurationMs(_ xberg.ExtractedDocument) uint64 {
	return 1
}
func (processor *pdfMetadataExtractor) Process(
	result xberg.ExtractedDocument,
	_ xberg.ExtractionConfig,
) error {
	if result.Metadata == nil {
		result.Metadata = &xberg.Metadata{}
	}
	if result.Metadata.Additional == nil {
		result.Metadata.Additional = make(map[string]json.RawMessage)
	}
	contentLength, err := json.Marshal(len(result.Content))
	if err != nil {
		return err
	}
	result.Metadata.Additional["pdf_content_length"] = contentLength
	result.Metadata.Additional["pdf_processor_version"] = json.RawMessage(`"1.0.0"`)
	processor.processedCount.Add(1)
	return nil
}

func main() {
	processor := &pdfMetadataExtractor{}
	if err := xberg.RegisterPostProcessor(processor); err != nil {
		log.Fatalf("register post-processor: %v", err)
	}
	defer func() {
		if err := xberg.UnregisterPostProcessor(processor.Name()); err != nil {
			log.Printf("unregister post-processor: %v", err)
		}
		log.Printf("PDFs processed: %d", processor.processedCount.Load())
	}()

	input := xberg.ExtractInputFromURI("document.pdf")
	result, err := xberg.Extract(*input, xberg.ExtractionConfig{})
	if err != nil {
		log.Fatalf("extract PDF: %v", err)
	}
	log.Printf("PDF MIME type: %s", result.Results[0].MimeType)
}
```
