import asyncio
from xberg import ExtractInput, extract, ExtractionConfig

config = ExtractionConfig(
    use_cache=True,
    cache_namespace="docs-example",
    cache_ttl_secs=7 * 86400,
)

async def main() -> None:
    print("First extraction (will be cached)...")
    result1 = await extract(ExtractInput(uri="document.pdf"), config)
    print(f"  - Content length: {len(result1.results[0].content)}")

    print("\nSecond extraction (served from cache)...")
    result2 = await extract(ExtractInput(uri="document.pdf"), config)
    print(f"  - Content length: {len(result2.results[0].content)}")

    print(f"\nResults are identical: {result1.results[0].content == result2.results[0].content}")

asyncio.run(main())
