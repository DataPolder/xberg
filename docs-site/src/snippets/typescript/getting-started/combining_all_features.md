```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  enableQualityProcessing: true,
  languageDetection: {
    enabled: true,
    detectMultiple: true,
  },
  tokenReduction: {
    level: "Moderate",
    preserveImportantWords: true,
  },
  chunking: {
    maxCharacters: 512,
    overlap: 50,
    embedding: {
      model: { type: "preset", name: "balanced" },
    },
  },
  keywords: {
    algorithm: "yake",
    maxKeywords: 10,
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
const result = output.results[0];

console.log(`Content length: ${result.content.length}`);
if (result.detectedLanguages) {
  console.log(`Languages: ${result.detectedLanguages.join(", ")}`);
}
if (result.chunks && result.chunks.length > 0) {
  console.log(`Chunks: ${result.chunks.length}`);
}
```
