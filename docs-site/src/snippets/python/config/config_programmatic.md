```python title="Python"
import asyncio
from xberg import (
    ExtractInput,
    extract,
    ExtractionConfig,
    OcrConfig,
    ChunkingConfig,
)

async def main() -> None:
    config: ExtractionConfig = ExtractionConfig(
        use_cache=True,
        ocr=OcrConfig(backend="tesseract", language="eng"),
        chunking=ChunkingConfig(max_characters=1000, overlap=200),
    )

    result = await extract(ExtractInput(uri="document.pdf"), config)
    content_length: int = len(result.results[0].content)
    print(f"Content length: {content_length}")

asyncio.run(main())
```
