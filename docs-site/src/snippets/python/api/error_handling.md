```python title="Python"
import asyncio
from xberg import ExtractInput, extract, ExtractionConfig, XbergError

async def main() -> None:
    config = ExtractionConfig()

    try:
        result = await extract(ExtractInput(uri="missing.pdf"), config)
    except XbergError as e:
        print(f"Extraction failed: {e}")
        raise

asyncio.run(main())
```
