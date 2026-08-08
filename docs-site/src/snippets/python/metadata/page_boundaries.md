```python title="Python"
import asyncio
from xberg import ExtractInput, extract, ExtractionConfig

async def main() -> None:
    result = await extract(ExtractInput(uri="document.pdf"), ExtractionConfig())

    document = result.results[0]
    if document.metadata.pages and document.metadata.pages.boundaries:
        boundaries = document.metadata.pages.boundaries
        content_bytes = document.content.encode("utf-8")

        for boundary in boundaries[:3]:
            page_bytes = content_bytes[boundary.byte_start:boundary.byte_end]
            page_text = page_bytes.decode("utf-8")

            print(f"Page {boundary.page_number}:")
            print(f"  Byte range: {boundary.byte_start}-{boundary.byte_end}")
            print(f"  Preview: {page_text[:100]}...")

asyncio.run(main())
```
