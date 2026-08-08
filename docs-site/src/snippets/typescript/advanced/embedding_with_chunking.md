```typescript title="TypeScript"
const config = {
  chunking: {
    maxCharacters: 1024,
    overlap: 100,
    embedding: {
      model: { type: "preset", name: "balanced" },
      normalize: true,
      batchSize: 32,
      showDownloadProgress: false,
    },
  },
};
```
