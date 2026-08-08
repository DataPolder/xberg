```typescript title="TypeScript"
import {
  registerOcrBackend,
  OcrBackendType,
  type OcrBackend,
  type OcrConfig,
  type ExtractedDocument,
} from "@xberg-io/xberg";

/**
 * Mock OCR backend for testing
 * Simulates OCR results without calling external service
 * @example
 * const backend = new MockOcrBackend();
 * registerOcrBackend(backend);
 */
class MockOcrBackend implements OcrBackend {
  private callCount: number = 0;

  name(): string {
    return "mock-ocr-backend";
  }

  backendType(): OcrBackendType {
    return OcrBackendType.Custom;
  }

  supportsLanguage(lang: string): boolean {
    return ["en", "de", "fr", "es"].includes(lang);
  }

  supportedLanguages(): string[] {
    return ["en", "de", "fr", "es"];
  }

  initialize(): void {
    console.log("Mock OCR backend initialized");
  }

  shutdown(): void {
    console.log("Mock OCR backend shutdown");
  }

  /**
   * Return mock OCR results based on image size
   */
  async processImage(
    imageBytes: Uint8Array,
    config?: OcrConfig | null,
  ): Promise<ExtractedDocument> {
    this.callCount++;

    const language = (config?.language ?? ["en"]).join("+");

    // Simulate OCR processing time
    await new Promise((resolve) => setTimeout(resolve, 100));

    const mockText = `This is mock OCR result for ${language} detected in ${imageBytes.length} bytes of image data.`;

    return {
      content: mockText,
      mimeType: "text/plain",
      metadata: { additional: { confidence: 0.95, language } },
    };
  }

  /**
   * Get backend statistics
   */
  getStats(): { callCount: number } {
    return { callCount: this.callCount };
  }
}

// Register mock OCR backend for testing
const mockBackend = new MockOcrBackend();
mockBackend.initialize();
registerOcrBackend(mockBackend);

// Usage in tests
// const output = await extract({ kind: ExtractInputKind.Uri, uri: "image.png" });
// console.log(mockBackend.getStats()); // { callCount: 1 }
```
