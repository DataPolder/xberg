```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const output = await extract({ kind: "uri", uri: "document.pdf" }, { pages: { extractPages: true } });
const result = output.results[0];

if (result.pages) {
  for (const page of result.pages) {
    console.log(`Page ${page.pageNumber}:`);
    console.log(`  Content: ${page.content.length} chars`);
    console.log(`  Tables: ${page.tables.length}`);
    console.log(`  Images: ${page.imageIndices.length}`);
  }
}
```
