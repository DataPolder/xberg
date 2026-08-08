```typescript title="TypeScript"
import { extract, type ExtractionConfig } from "@xberg-io/xberg";

const config: ExtractionConfig = {
  useCache: true,
  ocr: {
    backend: "tesseract",
    language: ["eng", "deu"],
    tesseractConfig: {
      psm: 6,
    },
  },
  chunking: {
    maxCharacters: 1000,
    overlap: 200,
  },
  enableQualityProcessing: true,
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
console.log(`Content length: ${output.results[0].content.length}`);
```
