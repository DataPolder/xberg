using Xberg;
using System;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

class WordCountPostProcessor : IPostProcessor
{
    public string Name => "word-count";
    public string Version => "1.0.0";
    public int Priority => 10;
    public ProcessingStage ProcessingStage => ProcessingStage.Middle;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) =>
        !string.IsNullOrEmpty(result.Content);

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

class CleanupPostProcessor : IPostProcessor
{
    public string Name => "text-cleanup";
    public string Version => "1.0.0";
    public int Priority => 5;
    public ProcessingStage ProcessingStage => ProcessingStage.Early;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) =>
        !string.IsNullOrEmpty(result.Content);

    public ulong EstimatedDurationMs(ExtractedDocument result) => 1;

    // ExtractedDocument.Content is immutable (init-only), so the cleaned
    // text is reported through Metadata.Additional rather than rewritten
    // in place.
    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        var cleaned = Regex.Replace(result.Content, @"\s+", " ").Trim();
        cleaned = Regex.Replace(cleaned, @"[^\w\s\.\,\!\?\-]", "");

        result.Metadata.Additional["cleaned_content"] = JsonSerializer.SerializeToElement(cleaned);
    }
}

class LanguageDetectionPostProcessor : IPostProcessor
{
    public string Name => "language-detection";
    public string Version => "1.0.0";
    public int Priority => 1;
    public ProcessingStage ProcessingStage => ProcessingStage.Early;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) =>
        !string.IsNullOrEmpty(result.Content);

    public ulong EstimatedDurationMs(ExtractedDocument result) => 1;

    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        var detectedLanguage = DetectLanguage(result.Content);
        result.Metadata.Additional["detected_language"] = JsonSerializer.SerializeToElement(detectedLanguage);
    }

    private string DetectLanguage(string text)
    {
        var commonEnglishWords = new[] { "the", "is", "and", "to", "of", "a", "in", "that" };
        var lowerText = text.ToLower();
        var matches = commonEnglishWords.Count(word =>
            Regex.IsMatch(lowerText, $@"\b{word}\b")
        );

        return matches > 5 ? "en" : "unknown";
    }
}

class Program
{
    static async Task Main()
    {
        var wordCountProcessor = new WordCountPostProcessor();
        var cleanupProcessor = new CleanupPostProcessor();
        var languageProcessor = new LanguageDetectionPostProcessor();

        PostProcessorRegistry.RegisterPostProcessor(wordCountProcessor);
        PostProcessorRegistry.RegisterPostProcessor(cleanupProcessor);
        PostProcessorRegistry.RegisterPostProcessor(languageProcessor);

        try
        {
            var config = ExtractionConfig.Default();
            var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

            Console.WriteLine($"Original content length: {result.Content.Length}");

            if (result.Metadata.Additional.TryGetValue("word_count", out var wc))
            {
                Console.WriteLine($"Word count: {wc}");
            }
            if (result.Metadata.Additional.TryGetValue("detected_language", out var lang))
            {
                Console.WriteLine($"Detected language: {lang}");
            }
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
