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
    var title = result.Metadata.Title;
    var author = result.Metadata.Authors;
    var pageCount = result.Metadata.Format.AsPdf.PageCount;
}
