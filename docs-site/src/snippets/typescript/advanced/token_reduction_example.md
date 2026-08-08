```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  tokenReduction: {
    level: "Moderate",
    preserveMarkdown: true,
  },
};

const output = await extract({ kind: "uri", uri: "verbose_document.pdf" }, config);

console.log(`Reduced content length: ${output.results[0].content?.length ?? 0} chars`);
```
