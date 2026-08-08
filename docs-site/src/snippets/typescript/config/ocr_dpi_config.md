```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  ocr: {
    backend: "tesseract",
    tesseractConfig: {
      preprocessing: {
        targetDpi: 300,
      },
    },
  },
};

const output = await extract({ kind: "uri", uri: "scanned.pdf" }, config);
console.log(`content length: ${output.results[0].content.length}`);
```
