```csharp title="C#"
using Xberg;
using System;

var processor = new UnregisterableProcessor();
PostProcessorRegistry.Register(processor);

Console.WriteLine("Processor registered");
var processors = XbergConverter.ListPostProcessors();
Console.WriteLine($"Active processors: {string.Join(", ", processors)}");

PostProcessorRegistry.Unregister(processor.Name);
Console.WriteLine("Processor unregistered");

processors = XbergConverter.ListPostProcessors();
Console.WriteLine($"Active processors: {string.Join(", ", processors)}");

public class UnregisterableProcessor : IPostProcessor
{
    public string Name => "removable-processor";
    public string Version => "1.0.0";
    public int Priority => 50;
    public ProcessingStage ProcessingStage => ProcessingStage.Middle;

    public void Initialize() { }
    public void Shutdown() { }

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        Console.WriteLine("Processing...");
    }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) => true;
    public ulong EstimatedDurationMs(ExtractedDocument result) => 10;
}
```
