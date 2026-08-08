import asyncio
from xberg import ExtractInput, PdfConfig, HierarchyConfig, ExtractionConfig, extract

async def main() -> None:
    # Example 1: Basic hierarchy extraction
    # Enabled with default k_clusters=6 for standard H1-H6 heading hierarchy.
    # Extract bounding box information for spatial layout awareness.
    hierarchy_config_basic = HierarchyConfig(
        enabled=True,
        k_clusters=6,  # Default: creates 6 font size clusters (H1-H6 structure)
        include_bbox=True,  # Include bounding box coordinates
    )

    pdf_config_basic = PdfConfig(hierarchy=hierarchy_config_basic)
    extraction_config_basic = ExtractionConfig(pdf_options=pdf_config_basic)

    result = await extract(ExtractInput(uri="document.pdf"), extraction_config_basic)

    # Example 2: Custom k_clusters for minimal structure
    # Use 3 clusters for simpler hierarchy with minimal structure.
    # Useful when you only need major section divisions (Main, Subsection, Detail).
    hierarchy_config_minimal = HierarchyConfig(
        enabled=True,
        k_clusters=3,  # Minimal clustering: just 3 levels
        include_bbox=True,
    )

    pdf_config_minimal = PdfConfig(hierarchy=hierarchy_config_minimal)
    extraction_config_minimal = ExtractionConfig(pdf_options=pdf_config_minimal)

    result = await extract(ExtractInput(uri="document.pdf"), extraction_config_minimal)

    # Example 3: Disabling bounding boxes for a smaller payload
    hierarchy_config_no_bbox = HierarchyConfig(
        enabled=True,
        k_clusters=6,
        include_bbox=False,
    )

    pdf_config_no_bbox = PdfConfig(hierarchy=hierarchy_config_no_bbox)
    extraction_config_no_bbox = ExtractionConfig(pdf_options=pdf_config_no_bbox)

    result = await extract(ExtractInput(uri="document.pdf"), extraction_config_no_bbox)
    print(len(result.results[0].content))


asyncio.run(main())

# Field descriptions:
#
# enabled: bool (default: True)
#   - Enable or disable hierarchy extraction
#   - When False, hierarchy structure is not analyzed
#
# k_clusters: int (default: 3, valid: 1-7)
#   - Number of font size clusters for hierarchy levels
#   - 6 provides H1-H6 heading levels with body text
#   - Higher values create more fine-grained hierarchy
#   - Lower values create simpler structure
#
# include_bbox: bool (default: True)
#   - Include bounding box coordinates in hierarchy blocks
#   - Required for spatial layout awareness and document structure
#   - Set to False only if space optimization is critical
