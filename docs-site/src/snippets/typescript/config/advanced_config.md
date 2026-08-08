```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  ocr: {
    backend: "tesseract",
    language: ["eng", "deu"],
  },
  chunking: {
    maxCharacters: 1000,
    overlap: 100,
  },
  tokenReduction: {
    level: "Aggressive",
  },
  languageDetection: {
    enabled: true,
    detectMultiple: true,
  },
  useCache: true,
  enableQualityProcessing: true,
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
const result = output.results[0];

if (result.chunks) {
  for (const chunk of result.chunks) {
    console.log(`Chunk: ${chunk.content.substring(0, 100)}...`);
  }
}

if (result.detectedLanguages) {
  console.log(`Languages: ${result.detectedLanguages.join(", ")}`);
}
```
