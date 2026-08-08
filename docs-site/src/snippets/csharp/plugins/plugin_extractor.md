```csharp title="C#"
using Xberg;
using System;
using System.Collections.Generic;

var extractor = new CustomTextExtractor();
DocumentExtractorRegistry.RegisterDocumentExtractor(extractor);

public class CustomTextExtractor : IDocumentExtractor
{
    public string Name => "custom-text-extractor";
    public string Version => "1.0.0";
    public int Priority => 50;
    public List<string> SupportedMimeTypes => new() { "text/plain" };

    public void Initialize()
    {
        Console.WriteLine("Custom text extractor initialized");
    }

    public void Shutdown()
    {
        Console.WriteLine("Custom text extractor shut down");
    }

    public bool CanHandle(string path, string mimeType) => mimeType == "text/plain";

    public ExtractedDocument Extract(ExtractInput input, ExtractionConfig config)
    {
        var bytes = input.Bytes ?? System.IO.File.ReadAllBytes(input.Uri!);
        var text = System.Text.Encoding.UTF8.GetString(bytes);

        return new ExtractedDocument
        {
            Content = text.ToUpper(),
            MimeType = "text/plain",
            Metadata = new Metadata(),
        };
    }
}
```
