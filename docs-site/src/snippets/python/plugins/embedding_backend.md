```python title="Python"
import asyncio
from collections.abc import Iterable

from xberg import (
    register_embedding_backend,
    ChunkingConfig,
    EmbeddingConfig,
    EmbeddingModelType,
    ExtractInput,
    ExtractionConfig,
    extract,
)
from sentence_transformers import SentenceTransformer

# Wrap an already-loaded embedder (e.g. sentence-transformers, llama-cpp-python,
# or a tuned ONNX session) so xberg can call back into it during chunking.
class MyEmbedder:
    def __init__(self):
        self._model = SentenceTransformer("BAAI/bge-base-en-v1.5")

    # Plugin trait hooks
    def name(self) -> str:
        return "my-embedder"

    def version(self) -> str:
        return "1.0.0"

    def initialize(self) -> None:
        # Optional warm-up; runs once at registration before dimensions() is cached.
        pass

    def shutdown(self) -> None:
        pass

    # EmbeddingBackend hooks
    def dimensions(self) -> int:
        # Captured once at registration; the dispatcher uses this for shape validation.
        return self._model.get_sentence_embedding_dimension()

    def embed(self, texts: list[str]) -> Iterable[Iterable[float]]:
        # Anything sequence-like works: a NumPy array, a list of NumPy vectors, or
        # plain nested lists. Return whatever your model already produces.
        return self._model.encode(texts)          # ndarray, no conversion
        # return [[0.1, 0.2, ...], [0.3, 0.4, ...]]   # nested lists, equally fine


# Register once at startup. Reference by name in config.
register_embedding_backend(MyEmbedder())

async def main() -> None:
    config = ExtractionConfig(
        chunking=ChunkingConfig(
            embedding=EmbeddingConfig(
                model=EmbeddingModelType.plugin("my-embedder"),
                # Optional: bound the wait on a hung backend (default: 60s; None disables)
                max_embed_duration_secs=30,
            ),
        ),
    )
    result = await extract(ExtractInput(uri="document.pdf"), config)
    for chunk in result.results[0].chunks or []:
        if chunk.embedding is not None:
            print(len(chunk.embedding))

asyncio.run(main())
```
