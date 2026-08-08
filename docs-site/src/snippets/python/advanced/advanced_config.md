```python title="Python"
import asyncio
from xberg import (
    ExtractInput,
    extract,
    ExtractionConfig,
    OcrConfig,
    ChunkingConfig,
    TokenReductionOptions,
    LanguageDetectionConfig,
)

async def main() -> None:
    config = ExtractionConfig(
        ocr=OcrConfig(backend="tesseract", language="eng+deu"),
        chunking=ChunkingConfig(max_characters=1000, overlap=100),
        token_reduction=TokenReductionOptions(mode="light"),
        language_detection=LanguageDetectionConfig(
            enabled=True, detect_multiple=True
        ),
        use_cache=True,
        enable_quality_processing=True,
    )

    result = await extract(ExtractInput(uri="document.pdf"), config)

    for chunk in result.results[0].chunks or []:
        print(f"Chunk: {chunk.content[:100]}")

    if result.results[0].detected_languages:
        print(f"Languages: {result.results[0].detected_languages}")

asyncio.run(main())
```
