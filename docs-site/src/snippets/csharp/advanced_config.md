```csharp title="C#"
using Xberg;

var config = new ExtractionConfig
{
    Ocr = new OcrConfig { Backend = "tesseract", Language = ["eng", "deu"] },
    Chunking = new ChunkingConfig { MaxCharacters = 1000, Overlap = 100 },
    TokenReduction = new TokenReductionConfig { Level = ReductionLevel.Moderate },
    LanguageDetection = new LanguageDetectionConfig
    {
        Enabled = true,
        DetectMultiple = true
    },
    UseCache = true,
    EnableQualityProcessing = true
};

var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

foreach (var chunk in result.Chunks)
{
    Console.WriteLine($"Chunk: {chunk.Content[..Math.Min(100, chunk.Content.Length)]}");
}

if (result.DetectedLanguages?.Count > 0)
{
    Console.WriteLine($"Languages: {string.Join(", ", result.DetectedLanguages)}");
}
```
