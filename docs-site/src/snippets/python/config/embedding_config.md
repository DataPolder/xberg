```python title="Python"
from xberg import ExtractionConfig, ChunkingConfig, EmbeddingConfig, EmbeddingModelType

config = ExtractionConfig(
    chunking=ChunkingConfig(
        max_characters=1000,
        embedding=EmbeddingConfig(
            model=EmbeddingModelType.preset("balanced"),
            batch_size=16,
            normalize=True,
            show_download_progress=True
        )
    )
)
```
