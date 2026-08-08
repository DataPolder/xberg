```go title="Go"
package main

import (
	"fmt"
	"log"
	"os"

	"github.com/xberg-io/xberg/packages/go"
)

// cloudOcrBackend implements xberg.OcrBackend by delegating recognition to a
// remote OCR service. Registering it makes it selectable via
// `OcrConfig.Backend = "cloud-ocr"`.
type cloudOcrBackend struct {
	endpoint string
}

func (cloudOcrBackend) Name() string      { return "cloud-ocr" }
func (cloudOcrBackend) Version() string   { return "1.0.0" }
func (cloudOcrBackend) Initialize() error { return nil }
func (cloudOcrBackend) Shutdown() error   { return nil }

func (cloudOcrBackend) BackendType() xberg.OcrBackendType { return xberg.OcrBackendTypeCustom }

func (cloudOcrBackend) SupportedLanguages() []string { return []string{"eng", "deu", "fra"} }

func (b cloudOcrBackend) SupportsLanguage(lang string) bool {
	for _, supported := range b.SupportedLanguages() {
		if supported == lang {
			return true
		}
	}
	return false
}

func (cloudOcrBackend) SupportsTableDetection() bool     { return false }
func (cloudOcrBackend) SupportsDocumentProcessing() bool { return false }
func (cloudOcrBackend) EmitsStructuredMarkdown() bool    { return false }

func (b cloudOcrBackend) ProcessImage(
	imageBytes []byte,
	config xberg.OcrConfig,
) (xberg.ExtractedDocument, error) {
	text, err := b.recognize(imageBytes, config.Language)
	if err != nil {
		return xberg.ExtractedDocument{}, fmt.Errorf("cloud-ocr: recognizing image: %w", err)
	}
	return xberg.ExtractedDocument{Content: text, MimeType: "text/plain"}, nil
}

func (b cloudOcrBackend) ProcessImageFile(
	path string,
	config xberg.OcrConfig,
) (xberg.ExtractedDocument, error) {
	imageBytes, err := os.ReadFile(path)
	if err != nil {
		return xberg.ExtractedDocument{}, fmt.Errorf("cloud-ocr: reading %s: %w", path, err)
	}
	return b.ProcessImage(imageBytes, config)
}

// ProcessDocument is only called when SupportsDocumentProcessing reports true.
func (cloudOcrBackend) ProcessDocument(
	path string,
	_config xberg.OcrConfig,
) (xberg.ExtractedDocument, error) {
	return xberg.ExtractedDocument{}, fmt.Errorf("cloud-ocr: document processing not supported")
}

// recognize posts the image to the remote service. Replace with a real client;
// read credentials from the environment, never hard-code them.
func (b cloudOcrBackend) recognize(imageBytes []byte, languages []string) (string, error) {
	if len(imageBytes) == 0 {
		return "", fmt.Errorf("empty image payload")
	}
	_ = languages
	return "recognized text", nil
}

func main() {
	backend := cloudOcrBackend{endpoint: os.Getenv("CLOUD_OCR_ENDPOINT")}
	if err := xberg.RegisterOcrBackend(backend); err != nil {
		log.Fatalf("register OCR backend failed: %v", err)
	}
	defer func() {
		if err := xberg.UnregisterOcrBackend("cloud-ocr"); err != nil {
			log.Printf("warning: unregister failed: %v", err)
		}
	}()

	config := xberg.ExtractionConfig{
		Ocr: &xberg.OcrConfig{
			Backend:  "cloud-ocr",
			Language: []string{"eng"},
		},
	}
	input := xberg.ExtractInputFromURI("scanned.pdf")
	result, err := xberg.Extract(*input, config)
	if err != nil {
		log.Fatalf("extract failed: %v", err)
	}

	log.Println("content length:", len(result.Results[0].Content))
}
```
