using Xberg;
using System;
using System.Collections.Generic;
using System.Net.Http;
using System.Threading.Tasks;

class CloudOcrBackend : IOcrBackend
{
    private readonly string _apiKey;
    private readonly string _apiEndpoint;
    private readonly HttpClient _httpClient;

    public CloudOcrBackend(string apiKey, string apiEndpoint = "https://api.example.com/ocr")
    {
        _apiKey = apiKey;
        _apiEndpoint = apiEndpoint;
        _httpClient = new HttpClient();
    }

    public string Name => "cloud-ocr-backend";
    public string Version => "1.0.0";
    public OcrBackendType BackendType => OcrBackendType.Custom;
    public List<string> SupportedLanguages => new() { "eng" };
    public bool SupportsTableDetection => false;
    public bool SupportsDocumentProcessing => false;
    public bool EmitsStructuredMarkdown => false;

    public void Initialize() { }
    public void Shutdown() => _httpClient.Dispose();

    public bool SupportsLanguage(string lang) => SupportedLanguages.Contains(lang);

    public ExtractedDocument ProcessImage(byte[] imageBytes, OcrConfig config)
    {
        var text = SendToCloudOcr(imageBytes);
        return new ExtractedDocument
        {
            Content = text,
            MimeType = "text/plain",
            Metadata = new Metadata(),
        };
    }

    public ExtractedDocument ProcessImageFile(string path, OcrConfig config) =>
        ProcessImage(System.IO.File.ReadAllBytes(path), config);

    public ExtractedDocument ProcessDocument(string path, OcrConfig config) =>
        throw new OcrException("cloud-ocr-backend does not support whole-document processing");

    private string SendToCloudOcr(byte[] imageBytes)
    {
        return Task.Run(async () =>
            {
                try
                {
                    using var content = new MultipartFormDataContent();
                    content.Add(new ByteArrayContent(imageBytes), "image");

                    var request = new HttpRequestMessage(
                        HttpMethod.Post,
                        _apiEndpoint
                    )
                    {
                        Content = content,
                        Headers =
                        {
                            { "Authorization", $"Bearer {_apiKey}" }
                        }
                    };

                    var response = await _httpClient.SendAsync(request);
                    response.EnsureSuccessStatusCode();

                    return await response.Content.ReadAsStringAsync();
                }
                catch (HttpRequestException ex)
                {
                    throw new OcrException($"Cloud OCR service error: {ex.Message}");
                }
        }).GetAwaiter().GetResult();
    }
}

class Program
{
    static async Task Main()
    {
        var backend = new CloudOcrBackend(apiKey: "your-api-key-here");
        OcrBackendRegistry.RegisterOcrBackend(backend);

        try
        {
            var config = new ExtractionConfig
            {
                Ocr = new OcrConfig
                {
                    Backend = "cloud-ocr-backend"
                }
            };

            var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];
            Console.WriteLine($"OCR text: {result.Content}");
        }
        catch (OcrException ex)
        {
            Console.WriteLine($"OCR error: {ex.Message}");
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
