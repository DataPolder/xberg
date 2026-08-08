```typescript title="TypeScript"
import {
  registerOcrBackend,
  OcrBackendType,
  type OcrBackend,
  type OcrConfig,
  type ExtractedDocument,
} from "@xberg-io/xberg";

/**
 * Custom OCR backend implementation
 * Allows integration with custom OCR services
 * @example
 * const backend = new CustomOcrBackend("http://localhost:8000");
 * registerOcrBackend(backend);
 */
class CustomOcrBackend implements OcrBackend {
  private apiUrl: string;

  constructor(apiUrl: string) {
    this.apiUrl = apiUrl;
  }

  name(): string {
    return "custom-ocr-backend";
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
    console.log(`Initializing custom OCR backend at ${this.apiUrl}`);
  }

  shutdown(): void {
    console.log("Shutting down custom OCR backend");
  }

  /**
   * Process image and extract text via OCR
   */
  async processImage(
    imageBytes: Uint8Array,
    config?: OcrConfig | null,
  ): Promise<ExtractedDocument> {
    const language = (config?.language ?? ["en"]).join("+");
    const formData = new FormData();
    const blob = new Blob([Buffer.from(imageBytes)], { type: "image/png" });
    formData.append("image", blob);
    formData.append("language", language);

    const response = await fetch(`${this.apiUrl}/ocr`, {
      method: "POST",
      body: formData,
    });

    if (!response.ok) {
      throw new Error(`OCR service failed: ${response.statusText}`);
    }

    const result = await response.json();
    return {
      content: result.text,
      mimeType: "text/plain",
      metadata: { additional: { confidence: result.confidence, language } },
    };
  }
}

// Register custom OCR backend
const backend = new CustomOcrBackend("http://localhost:8000");
backend.initialize();
registerOcrBackend(backend);
```
