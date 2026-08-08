```python title="Python"
import asyncio
from xberg import ExtractInput, extract, ExtractionConfig, LanguageDetectionConfig

async def main() -> None:
    config = ExtractionConfig(
        language_detection=LanguageDetectionConfig(
            enabled=True,
            min_confidence=0.8,
            detect_multiple=True,
        ),
    )

    result = await extract(ExtractInput(uri="multilingual_document.pdf"), config)

    if result.results[0].detected_languages:
        print(f"Detected languages: {', '.join(result.results[0].detected_languages)}")

asyncio.run(main())
```
