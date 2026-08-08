```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  chunking: {
    maxCharacters: 1000,
    embedding: {
      model: { type: "preset", name: "quality" },
    },
  },
};

const output = await extract({ kind: "uri", uri: "document.pdf" }, config);
const result = output.results[0];
if (result.chunks && result.chunks.length > 0) {
  console.log(`Chunk embeddings: ${result.chunks[0].embedding?.length ?? 0} dimensions`);
}
```
