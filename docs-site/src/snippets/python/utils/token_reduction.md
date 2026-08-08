```python title="Python"
import asyncio
from xberg import ExtractInput, ExtractionConfig, TokenReductionOptions, extract

async def main() -> None:
    config: ExtractionConfig = ExtractionConfig(
        token_reduction=TokenReductionOptions(
            mode="moderate", preserve_important_words=True
        )
    )
    result = await extract(ExtractInput(uri="document.pdf"), config)
    print(f"Content length: {len(result.results[0].content)}")

asyncio.run(main())
```
