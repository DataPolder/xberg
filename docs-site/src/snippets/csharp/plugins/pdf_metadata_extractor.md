```csharp title="C#"
using Xberg;
using System;
using System.Text.Json;

var enricher = new PdfMetadataEnricher();
PostProcessorRegistry.Register(enricher);

public class PdfMetadataEnricher : IPostProcessor
{
    private int _processedCount = 0;

    public string Name => "pdf-metadata-enricher";
    public string Version => "1.0.0";
    public int Priority => 50;
    public ProcessingStage ProcessingStage => ProcessingStage.Early;

    public void Initialize()
    {
        Console.WriteLine("PDF metadata enricher initialized");
        _processedCount = 0;
    }

    public void Shutdown()
    {
        Console.WriteLine($"PDF metadata enricher processed {_processedCount} documents");
    }

    // ExtractedDocument.Metadata and Metadata.Authors are init-only, so
    // enrichment is reported through Metadata.Additional instead of
    // rewriting the document/metadata in place.
    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        if (result.MimeType == "application/pdf")
        {
            _processedCount++;
            if (result.Metadata.Authors is null or { Count: 0 })
            {
                result.Metadata.Additional["author_fallback"] = JsonSerializer.SerializeToElement("Unknown");
            }
        }
    }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config)
    {
        return result.MimeType == "application/pdf";
    }

    public ulong EstimatedDurationMs(ExtractedDocument result)
    {
        return 50;
    }
}
```
