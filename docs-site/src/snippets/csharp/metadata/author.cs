using Xberg;

var config = new ExtractionConfig
{
    PdfOptions = new PdfConfig
    {
        ExtractMetadata = true
    }
};

var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

if (result.Metadata != null)
{
    var authors = result.Metadata.Authors;
    Console.WriteLine($"Author: {string.Join(", ", authors)}");
}
