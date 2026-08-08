```csharp title="C#"
using Xberg;
using System;
using System.Text.Json;

var processor = new WordCountProcessor();
PostProcessorRegistry.Register(processor);

public class WordCountProcessor : IPostProcessor
{
    public string Name => "word-count";
    public string Version => "1.0.0";
    public int Priority => 50;
    public ProcessingStage ProcessingStage => ProcessingStage.Early;

    public void Initialize()
    {
        Console.WriteLine("Word count processor initialized");
    }

    public void Shutdown()
    {
        Console.WriteLine("Word count processor shut down");
    }

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        var wordCount = CountWords(result.Content);
        result.Metadata.Additional["word_count"] = JsonSerializer.SerializeToElement(wordCount);

        Console.WriteLine($"Document contains {wordCount} words");
    }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config)
    {
        return !string.IsNullOrEmpty(result.Content);
    }

    public ulong EstimatedDurationMs(ExtractedDocument result)
    {
        return 5;
    }

    private int CountWords(string content)
    {
        if (string.IsNullOrWhiteSpace(content))
            return 0;

        return content.Split(new[] { ' ', '\t', '\n', '\r' }, System.StringSplitOptions.RemoveEmptyEntries).Length;
    }
}
```
