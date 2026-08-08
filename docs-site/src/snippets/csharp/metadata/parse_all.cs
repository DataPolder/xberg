using Xberg;

var config = new ExtractionConfig
{
    PdfOptions = new PdfConfig
    {
        ExtractMetadata = true
    }
};

var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

if (result.Metadata?.Format?.AsPdf != null)
{
    var pdfMeta = result.Metadata.Format.AsPdf;
    Console.WriteLine($"Pages: {pdfMeta.PageCount}");
    Console.WriteLine($"Author: {string.Join(", ", result.Metadata.Authors)}");
    Console.WriteLine($"Title: {result.Metadata.Title}");
    Console.WriteLine($"Subject: {result.Metadata.Subject}");
    Console.WriteLine($"Created: {result.Metadata.CreatedAt}");
}

var htmlResult = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("page.html"), config)).Results[0];
if (htmlResult.Metadata?.Format?.AsHtml != null)
{
    var htmlMeta = htmlResult.Metadata.Format.AsHtml;
    Console.WriteLine($"Title: {htmlMeta.Title}");
    Console.WriteLine($"Description: {htmlMeta.Description}");
    if (htmlMeta.OpenGraph != null && htmlMeta.OpenGraph.ContainsKey("image"))
    Console.WriteLine($"Open Graph Image: {htmlMeta.OpenGraph["image"]}");
}
