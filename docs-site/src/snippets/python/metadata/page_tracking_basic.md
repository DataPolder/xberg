import asyncio
from xberg import extract, ExtractInput, ExtractionConfig, PageConfig

async def main() -> None:
    config = ExtractionConfig(
        pages=PageConfig(extract_pages=True)
    )

    result = await extract(ExtractInput(uri="document.pdf"), config)

    if result.results[0].pages:
        for page in result.results[0].pages:
            print(f"Page {page.page_number}:")
            print(f" Content: {len(page.content)} chars")
            print(f" Tables: {len(page.tables)}")
            print(f" Images: {len(page.image_indices)}")

asyncio.run(main())
