```python title="Python"
import asyncio
from xberg import (
    ExtractInput,
    extract,
    ExtractionConfig,
    ChunkingConfig,
    EmbeddingConfig,
    EmbeddingModelType,
    LanguageDetectionConfig,
    TokenReductionOptions,
)

async def main() -> None:
    config: ExtractionConfig = ExtractionConfig(
        enable_quality_processing=True,
        language_detection=LanguageDetectionConfig(enabled=True),
        token_reduction=TokenReductionOptions(mode="moderate"),
        chunking=ChunkingConfig(
            max_characters=512,
            overlap=50,
            embedding=EmbeddingConfig(
                model=EmbeddingModelType.preset("balanced"), normalize=True
            ),
        ),
    )
    result = await extract(ExtractInput(uri="document.pdf"), config)
    quality = result.results[0].quality_score or 0
    print(f"Quality: {quality:.2f}")
    print(f"Languages: {result.results[0].detected_languages}")
    if result.results[0].chunks:
        print(f"Chunks: {len(result.results[0].chunks)}")

asyncio.run(main())
```
