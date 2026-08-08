```csharp title="C#"
using Xberg;
using System;
using System.Collections.Generic;

var extractor = new JsonDocumentExtractor();
DocumentExtractorRegistry.RegisterDocumentExtractor(extractor);

public class JsonDocumentExtractor : IDocumentExtractor
{
    public string Name => "json-extractor";
    public string Version => "1.0.0";
    public int Priority => 50;
    public List<string> SupportedMimeTypes => new() { "application/json", "text/json" };

    public void Initialize()
    {
        Console.WriteLine("JSON extractor initialized");
    }

    public void Shutdown()
    {
        Console.WriteLine("JSON extractor shut down");
    }

    public bool CanHandle(string path, string mimeType) =>
        mimeType == "application/json" || mimeType == "text/json";

    public ExtractedDocument Extract(ExtractInput input, ExtractionConfig config)
    {
        var bytes = input.Bytes ?? System.IO.File.ReadAllBytes(input.Uri!);
        var json = System.Text.Encoding.UTF8.GetString(bytes);

        return new ExtractedDocument
        {
            Content = json,
            MimeType = input.MimeType ?? "application/json",
            Metadata = new Metadata(),
        };
    }
}
```
