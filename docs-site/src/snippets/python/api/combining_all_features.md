```python title="Python"
import asyncio
from xberg import (
    ExtractInput,
    ExtractionConfig,
    OcrConfig,
    ChunkingConfig,
    ChunkerType,
    ImageExtractionConfig,
    extract,
)

async def main() -> None:
    config = ExtractionConfig(
        # OCR: extract text from images, fallback to Tesseract
        ocr=OcrConfig(
            enabled=True,
            backend="tesseract",
            language="eng",
        ),
        # Chunking: semantic markdown chunks of ~800 chars, 100-char overlap
        chunking=ChunkingConfig(
            max_characters=800,
            overlap=100,
            chunker_type=ChunkerType.MARKDOWN,
            prepend_heading_context=True,
        ),
        # Output: Markdown format with document structure preserved
        output_format="markdown",
        include_document_structure=True,
        # Images: extract embedded images
        images=ImageExtractionConfig(
            extract_images=True,
        ),
        # Cache extracted results on disk
        use_cache=True,
    )

    result = await extract(ExtractInput(uri="report.pdf"), config)
    document = result.results[0]

    print(f"Content ({len(document.content)} chars):")
    print(document.content[:200])

    if document.chunks:
        print(f"\nChunks: {len(document.chunks)}")

    print(f"Tables: {len(document.tables)}")

    if document.detected_languages:
        print(f"Languages: {document.detected_languages}")

    if document.extraction_method:
        print(f"Extraction method: {document.extraction_method}")

asyncio.run(main())
```
