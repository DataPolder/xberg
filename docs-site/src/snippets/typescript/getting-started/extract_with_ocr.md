```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  forceOcr: true,
  ocr: {
    backend: "tesseract",
    language: ["eng"],
  },
};

const output = await extract({ kind: "uri", uri: "scanned.pdf" }, config);
const result = output.results[0];

console.log(result.content);
console.log(`Detected Languages: ${result.detectedLanguages?.join(", ") ?? "none"}`);
```
