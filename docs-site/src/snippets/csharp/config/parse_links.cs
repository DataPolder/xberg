using Xberg;

var config = new ExtractionConfig
{
    UseCache = true
};

var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.html"), config)).Results[0];

if (result.Metadata?.Format?.AsHtml?.Links != null)
{
    foreach (var link in result.Metadata.Format.AsHtml.Links)
    {
        var text = link.Text;
        var url = link.Href;
    }
}
