```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  ocr: {
    backend: "tesseract",
    language: ["eng"],
    tesseractConfig: {
      preprocessing: {
        targetDpi: 300,
        denoise: true,
        deskew: true,
        contrastEnhance: true,
        binarizationMethod: "otsu",
      },
    },
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
console.log(`content length: ${output.results[0].content.length}`);
```
