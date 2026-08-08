```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  postprocessor: {
    enabled: true,
    enabledProcessors: ["deduplication", "whitespace_normalization"],
    disabledProcessors: ["mojibake_fix"],
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
console.log(output.results[0].content);
```
