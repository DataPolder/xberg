using Xberg;

Console.WriteLine("Xberg import successful");

var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), ExtractionConfig.Default())).Results[0];
Console.WriteLine($"Extraction successful: {result.Content.Length > 0}");
