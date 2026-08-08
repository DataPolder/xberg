using Xberg;
using System;
using System.Text.Json;

class WordCountPostProcessor : IPostProcessor
{
    public string Name => "word-count";
    public string Version => "1.0.0";
    public int Priority => 10;
    public ProcessingStage ProcessingStage => ProcessingStage.Middle;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) => true;
    public ulong EstimatedDurationMs(ExtractedDocument result) => 1;

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        var wordCount = result.Content.Split(
            new[] { ' ', '\n', '\r', '\t' },
            StringSplitOptions.RemoveEmptyEntries
        ).Length;

        result.Metadata.Additional["word_count"] = JsonSerializer.SerializeToElement(wordCount);
    }
}

class SentimentPostProcessor : IPostProcessor
{
    public string Name => "sentiment-analyzer";
    public string Version => "1.0.0";
    public int Priority => 5;
    public ProcessingStage ProcessingStage => ProcessingStage.Late;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) => true;
    public ulong EstimatedDurationMs(ExtractedDocument result) => 1;

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        var sentiment = AnalyzeSentiment(result.Content);
        result.Metadata.Additional["sentiment"] = JsonSerializer.SerializeToElement(sentiment);
    }

    private string AnalyzeSentiment(string text)
    {
        return text.Length > 0 ? "neutral" : "unknown";
    }
}

class Program
{
    static async System.Threading.Tasks.Task Main()
    {
        var wordCountProcessor = new WordCountPostProcessor();
        var sentimentProcessor = new SentimentPostProcessor();

        PostProcessorRegistry.RegisterPostProcessor(wordCountProcessor);
        PostProcessorRegistry.RegisterPostProcessor(sentimentProcessor);

        try
        {
            var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), ExtractionConfig.Default())).Results[0];

            if (result.Metadata.Additional.TryGetValue("word_count", out var wordCount))
            {
                Console.WriteLine($"Word count: {wordCount}");
            }
            if (result.Metadata.Additional.TryGetValue("sentiment", out var sentiment))
            {
                Console.WriteLine($"Sentiment: {sentiment}");
            }
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
