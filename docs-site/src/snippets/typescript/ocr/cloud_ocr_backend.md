```typescript title="TypeScript"
import {
  registerOcrBackend,
  extract,
  OcrBackendType,
  type OcrBackend,
  type OcrConfig,
} from "@xberg-io/xberg";

const supportedLangs = ["eng", "deu", "fra"];

const cloudBackend: OcrBackend = {
  name: () => "cloud-ocr",
  version: () => "1.0.0",
  initialize: () => {},
  shutdown: () => {},
  processImage: async (imageBytes: Uint8Array, config?: OcrConfig | null) => {
    // Call your cloud OCR API with imageBytes and config.language.
    return { content: "Extracted text", mimeType: "text/plain" };
  },
  supportsLanguage: (lang: string) => supportedLangs.includes(lang),
  backendType: () => OcrBackendType.Custom,
  supportedLanguages: () => supportedLangs,
};

registerOcrBackend(cloudBackend);

const output = await extract({
  kind: "uri",
  uri: "scanned.pdf",
}, {
  ocr: {
    backend: "cloud-ocr",
    language: ["eng"],
  },
});
console.log(output.results[0].content);
```
