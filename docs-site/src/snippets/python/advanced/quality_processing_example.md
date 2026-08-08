```python title="Python"
import asyncio
from xberg import ExtractInput, ExtractionConfig, extract


async def main() -> None:
    config: ExtractionConfig = ExtractionConfig(
        enable_quality_processing=True,
    )

    result = await extract(ExtractInput(uri="scanned_document.pdf"), config)

    if result.results[0].quality_score is not None:
        if result.results[0].quality_score < 0.5:
            print(f"Warning: Low quality extraction ({result.results[0].quality_score:.2f})")
        else:
            print(f"Quality score: {result.results[0].quality_score:.2f}")


asyncio.run(main())
```
