using Xberg;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

class JsonDocumentExtractor : IDocumentExtractor
{
    public string Name => "json-extractor";
    public string Version => "1.0.0";
    public int Priority => 60;
    public List<string> SupportedMimeTypes => new() { "application/json" };

    public void Initialize() { }
    public void Shutdown() { }

    public bool CanHandle(string path, string mimeType) =>
        mimeType == "application/json" || path.EndsWith(".json", StringComparison.OrdinalIgnoreCase);

    public ExtractedDocument Extract(ExtractInput input, ExtractionConfig config)
    {
        var bytes = input.Bytes ?? throw new ParsingException("JSON extractor requires bytes input");
        try
        {
            var jsonContent = System.Text.Encoding.UTF8.GetString(bytes);
            var document = JsonDocument.Parse(jsonContent);
            var text = ExtractText(document.RootElement);

            return new ExtractedDocument
            {
                Content = text,
                MimeType = "application/json",
                Metadata = new Metadata(),
                Tables = new List<Table>()
            };
        }
        catch (JsonException ex)
        {
            throw new ParsingException($"Failed to parse JSON: {ex.Message}");
        }
    }

    private static string ExtractText(JsonElement element)
    {
        return element.ValueKind switch
        {
            JsonValueKind.String => element.GetString() + "\n",
            JsonValueKind.Array => string.Concat(
                element.EnumerateArray().Select(ExtractText)
            ),
            JsonValueKind.Object => string.Concat(
                element.EnumerateObject()
                .Select(p => ExtractText(p.Value))
            ),
            _ => ""
        };
    }
}

class Program
{
    static void Main()
    {
        try
        {
            var extractor = new JsonDocumentExtractor();
            DocumentExtractorRegistry.RegisterDocumentExtractor(extractor);

            var jsonData = new { message = "Hello, world!", timestamp = DateTime.UtcNow };
            var jsonBytes = System.Text.Encoding.UTF8.GetBytes(
                JsonSerializer.Serialize(jsonData)
            );

            var input = ExtractInput.FromBytes(jsonBytes, "application/json", null);
            var result = extractor.Extract(input, ExtractionConfig.Default());

            Console.WriteLine($"Extracted: {result.Content}");
            Console.WriteLine($"MIME type: {result.MimeType}");
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
