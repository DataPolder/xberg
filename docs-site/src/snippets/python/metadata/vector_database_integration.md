```python title="Python"
import asyncio
from xberg import ExtractInput, extract, ExtractionConfig, ChunkingConfig, EmbeddingConfig, EmbeddingModelType

async def main() -> None:
    config = ExtractionConfig(
        chunking=ChunkingConfig(
            max_characters=512,
            overlap=50,
            embedding=EmbeddingConfig(
                model=EmbeddingModelType.preset("balanced"),
                normalize=True,
                batch_size=32,
            ),
        ),
    )

    result = await extract(ExtractInput(uri="document.pdf"), config)

    records: list[dict] = []
    if result.results[0].chunks:
        for index, chunk in enumerate(result.results[0].chunks):
            if chunk.embedding is None:
                continue
            records.append({
                "id": f"document_chunk_{index}",
                "content": chunk.content,
                "embedding": chunk.embedding,
                "metadata": {
                    "document_id": "document.pdf",
                    "chunk_index": index,
                    "content_length": len(chunk.content),
                },
            })

    print(f"Prepared {len(records)} vector records")

asyncio.run(main())
```
