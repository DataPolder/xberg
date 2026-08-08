```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  pdfOptions: {
    extractImages: true,
    extractMetadata: true,
    passwords: ["password1", "password2"],
    hierarchy: { enabled: true, kClusters: 6, includeBbox: true },
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
console.log(output.results[0].content);
```
