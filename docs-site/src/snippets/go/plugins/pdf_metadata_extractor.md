```go title="Go"
package main

import (
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
	_ xberg.ExtractedDocument,
	_ xberg.ExtractionConfig,
) error {
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
