```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

interface VectorRecord {
  id: string;
  content: string;
  embedding: number[];
  metadata: Record<string, string>;
}

async function extractAndVectorize(
  documentPath: string,
  documentId: string,
): Promise<VectorRecord[]> {
  const config = {
    chunking: {
      maxCharacters: 512,
      overlap: 50,
      embedding: {
        model: { type: "preset", name: "balanced" },
        normalize: true,
        batchSize: 32,
      },
    },
  };

  const output = await extract({ kind: "uri", uri: documentPath }, config);
  const result = output.results[0];

  const records: VectorRecord[] = [];
  if (result.chunks) {
    result.chunks.forEach((chunk, index) => {
      if (chunk.embedding) {
        records.push({
          id: `${documentId}_chunk_${index}`,
          content: chunk.content,
          embedding: chunk.embedding,
          metadata: {
            document_id: documentId,
            chunk_index: index.toString(),
            content_length: chunk.content.length.toString(),
          },
        });
      }
    });
  }

  return records;
}
```
