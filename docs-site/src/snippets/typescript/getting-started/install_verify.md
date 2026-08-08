```typescript title="TypeScript"
import { extract, ExtractInputKind } from "@xberg-io/xberg";

const output = await extract({
  kind: ExtractInputKind.Uri,
  uri: "document.pdf",
});
console.log(`Extraction successful: ${output.errors?.length === 0}`);
```
