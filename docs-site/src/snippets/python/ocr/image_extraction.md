```python title="Python"
import asyncio
from xberg import ExtractInput, extract, ExtractionConfig, ImageExtractionConfig

async def main() -> None:
    config: ExtractionConfig = ExtractionConfig(
        images=ImageExtractionConfig(
            extract_images=True,
            target_dpi=200,
            max_image_dimension=2048,
            inject_placeholders=True,  # set to False to extract images without markdown references
            auto_adjust_dpi=True,
        )
    )

    result = await extract(ExtractInput(uri="document.pdf"), config)

    print(f"Content length: {len(result.results[0].content)} characters")

asyncio.run(main())
```
