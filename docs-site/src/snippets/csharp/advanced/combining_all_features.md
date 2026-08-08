```csharp title="C#"
using System;
using System.Threading.Tasks;
using Xberg;

async Task RunRagPipeline()
{
    var config = new ExtractionConfig
    {
        EnableQualityProcessing = true,

        LanguageDetection = new LanguageDetectionConfig
        {
            Enabled = true,
            DetectMultiple = true,
            MinConfidence = 0.8,
        },

        TokenReduction = new TokenReductionConfig
        {
            Level = ReductionLevel.Moderate,
            PreserveImportantWords = true,
        },

        Chunking = new ChunkingConfig
        {
            MaxCharacters = 512,
            Overlap = 50,
            Embedding = new EmbeddingConfig
            {
                Model = new EmbeddingModelType.Preset("balanced"),
            },
        },

        Keywords = new KeywordConfig
        {
            Algorithm = KeywordAlgorithm.Yake,
            MaxKeywords = 10,
        },
    };

    var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

    Console.WriteLine($"Content length: {result.Content.Length} characters");

    if (result.DetectedLanguages?.Count > 0)
    {
        Console.WriteLine($"Languages: {string.Join(", ", result.DetectedLanguages)}");
    }

    if (result.Chunks?.Count > 0)
    {
        Console.WriteLine($"Total chunks: {result.Chunks.Count}");
        var firstChunk = result.Chunks[0];
        Console.WriteLine($"First chunk tokens: {firstChunk.Metadata.TokenCount}");
        if (firstChunk.Embedding?.Count > 0)
        {
            Console.WriteLine($"Embedding dimensions: {firstChunk.Embedding.Count}");
        }
    }

    Console.WriteLine($"Quality score: {result.QualityScore}");

    if (result.ExtractedKeywords?.Count > 0)
    {
        Console.WriteLine($"Keywords: {string.Join(", ", result.ExtractedKeywords)}");
    }
}

await RunRagPipeline();
```
