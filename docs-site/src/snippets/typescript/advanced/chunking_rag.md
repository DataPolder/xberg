```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  chunking: {
    maxCharacters: 500,
    overlap: 50,
    embedding: {
      model: { type: "preset", name: "balanced" },
      normalize: true,
    },
  },
};

const output = await extract({ kind: "uri", uri: "research_paper.pdf" }, config);

for (const chunk of output.results[0].chunks ?? []) {
  console.log(`Chunk ${chunk.metadata.chunkIndex + 1}/${chunk.metadata.totalChunks}`);
  console.log(`Position: ${chunk.metadata.byteStart}-${chunk.metadata.byteEnd}`);
  console.log(`Content: ${chunk.content.slice(0, 100)}...`);
  if (chunk.embedding) {
    console.log(`Embedding: ${chunk.embedding.length} dimensions`);
  }
}
```
