```csharp title="C#"
using Xberg;
using System;

public class PdfOnlyProcessor : IPostProcessor
{
    public string Name => "pdf-only-processor";
    public string Version => "1.0.0";
    public int Priority => 50;
    public ProcessingStage ProcessingStage => ProcessingStage.Middle;

    public void Initialize()
    {
    }

    public void Shutdown()
    {
    }

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        if (result.MimeType != "application/pdf")
        {
            Console.WriteLine($"Skipping non-PDF: {result.MimeType}");
        }
    }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config)
    {
        return result.MimeType == "application/pdf";
    }

    public ulong EstimatedDurationMs(ExtractedDocument result)
    {
        return 10;
    }
}

class Program
{
    static void Main()
    {
        var processor = new PdfOnlyProcessor();
        PostProcessorRegistry.Register(processor);
    }
}
```
