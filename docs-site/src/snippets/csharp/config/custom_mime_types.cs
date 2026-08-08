using Xberg;

var config = new ExtractionConfig
{
    UseCache = true,
    EnableQualityProcessing = true
};

var fileBytes = await File.ReadAllBytesAsync("document.pdf");

var result = (await XbergConverter.ExtractAsync(
    ExtractInput.FromBytes(fileBytes, "application/pdf", "document.pdf"),
    config
)).Results[0];

var mimeType = result.MimeType;
