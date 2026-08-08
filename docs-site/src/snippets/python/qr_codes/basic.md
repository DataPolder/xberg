```python title="Python"
import asyncio
from xberg import ExtractInput, extract, ExtractionConfig

async def main() -> None:
    config = ExtractionConfig(qr_codes=True)
    result = await extract(ExtractInput(uri="ticket.pdf"), config)
    for image in result.results[0].images or []:
        for qr in image.qr_codes or []:
            print(qr.payload)

asyncio.run(main())
```
