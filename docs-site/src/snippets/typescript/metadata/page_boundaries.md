```typescript title="TypeScript"
import { extract } from '@xberg-io/xberg';

const output = await extract({
  kind: "uri",
  uri: 'document.pdf',
});
const result = output.results[0];

if (result.metadata?.pages?.boundaries) {
  const encoder = new TextEncoder();
  const contentBytes = encoder.encode(result.content);

  for (const boundary of result.metadata.pages.boundaries.slice(0, 3)) {
    const pageBytes = contentBytes.slice(boundary.byteStart, boundary.byteEnd);
    const pageText = new TextDecoder().decode(pageBytes);

    console.log(`Page ${boundary.pageNumber}:`);
    console.log(`  Byte range: ${boundary.byteStart}-${boundary.byteEnd}`);
    console.log(`  Preview: ${pageText.substring(0, 100)}...`);
  }
}
```
