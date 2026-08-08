```csharp title="C#"
using Xberg;
using System;

var processor = new LoggingPostProcessor();
PostProcessorRegistry.Register(processor);

public class LoggingPostProcessor : IPostProcessor
{
    public string Name => "logging-processor";
    public string Version => "1.0.0";
    public int Priority => 10;
    public ProcessingStage ProcessingStage => ProcessingStage.Early;

    public void Initialize()
    {
        Console.WriteLine("Logging post-processor initialized");
    }

    public void Shutdown()
    {
        Console.WriteLine("Logging post-processor shut down");
    }

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        Console.WriteLine($"Processing: {result.MimeType}, Content length: {result.Content.Length}");

        if (string.IsNullOrEmpty(result.Content))
        {
            Console.WriteLine("Warning: Extracted content is empty");
        }
    }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config)
    {
        return true;
    }

    public ulong EstimatedDurationMs(ExtractedDocument result)
    {
        return 1;
    }
}
```
