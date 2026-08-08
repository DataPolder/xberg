```typescript title="TypeScript"
import { extract, ExtractInputKind } from "@xberg-io/xberg";
import { version } from "@xberg-io/xberg/package.json" with { type: "json" };

console.log(`Xberg version: ${version}`);

const output = await extract({
  kind: ExtractInputKind.Uri,
  uri: "document.pdf",
});
console.log(`Extraction successful: ${output.errors?.length === 0}`);
```
