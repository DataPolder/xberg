```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const output = await extract(
  { kind: "uri", uri: "document.pdf" },
  { chunking: { maxCharacters: 500, overlap: 50 }, pages: { extractPages: true } },
);
const result = output.results[0];

if (result.chunks) {
  for (const chunk of result.chunks) {
    if (chunk.metadata.firstPage) {
      const pageRange =
        chunk.metadata.firstPage === chunk.metadata.lastPage
          ? `Page ${chunk.metadata.firstPage}`
          : `Pages ${chunk.metadata.firstPage}-${chunk.metadata.lastPage}`;

      console.log(`Chunk: ${chunk.content.substring(0, 50)}... (${pageRange})`);
    }
  }
}
```
