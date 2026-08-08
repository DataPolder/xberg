```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  useCache: true,
  enableQualityProcessing: true,
  forceOcr: false,
  ocr: {
    backend: "tesseract",
    language: ["eng", "fra"],
    tesseractConfig: {
      psm: 3,
      enableTableDetection: true,
    },
  },
  pdfOptions: {
    extractImages: true,
    extractMetadata: true,
  },
  images: {
    extractImages: true,
    targetDpi: 150,
    maxImageDimension: 2048,
  },
  chunking: {
    maxCharacters: 1000,
    overlap: 200,
    embedding: {
      model: { type: "preset", name: "balanced" },
    },
  },
  tokenReduction: {
    level: "Moderate",
    preserveImportantWords: true,
  },
  languageDetection: {
    enabled: true,
    minConfidence: 0.8,
    detectMultiple: false,
  },
  postprocessor: {
    enabled: true,
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
console.log(`Extracted content length: ${output.results[0].content.length}`);
```
