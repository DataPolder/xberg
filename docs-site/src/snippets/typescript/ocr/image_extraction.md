```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  images: {
    extractImages: true,
    targetDpi: 200,
    maxImageDimension: 2048,
    injectPlaceholders: true, // set to false to extract images without markdown references
    autoAdjustDpi: true,
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
console.log(`content length: ${output.results[0].content.length}`);
```
