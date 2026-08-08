using Xberg;

class Program
{
    static async Task Main()
    {
        try
        {
            var config = new ExtractionConfig
            {
                UseCache = true,
                EnableQualityProcessing = true,
                ForceOcr = false,

                Ocr = new OcrConfig
                {
                    Backend = "tesseract",
                    Language = ["eng", "fra"],
                    TesseractConfig = new TesseractConfig
                    {
                        Psm = 3,
                        Oem = 3,
                        MinConfidence = 0.8,
                        Preprocessing = new ImagePreprocessingConfig
                        {
                            TargetDpi = 300,
                            Denoise = true,
                            Deskew = true,
                            ContrastEnhance = true
                        },
                        EnableTableDetection = true
                    }
                },

                PdfOptions = new PdfConfig
                {
                    ExtractImages = true,
                    ExtractMetadata = true
                },

                Images = new ImageExtractionConfig
                {
                    ExtractImages = true,
                    TargetDpi = 150,
                    MaxImageDimension = 4096
                },

                Chunking = new ChunkingConfig
                {
                    MaxCharacters = 1000,
                    Overlap = 200,
                    Preset = "default"
                },

                TokenReduction = new TokenReductionConfig
                {
                    Level = ReductionLevel.Moderate,
                    PreserveImportantWords = true
                },

                LanguageDetection = new LanguageDetectionConfig
                {
                    Enabled = true,
                    MinConfidence = 0.8,
                    DetectMultiple = false
                },

                Postprocessor = new PostProcessorConfig
                {
                    Enabled = true
                }
            };

            var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri(
                "document.pdf"), config
            )).Results[0];

            Console.WriteLine($"Content length: {result.Content.Length}");
            Console.WriteLine($"MIME type: {result.MimeType}");

            if (result.Tables.Any())
            {
                Console.WriteLine($"Found {result.Tables.Count} tables");
            }

            if (result.Chunks?.Any() == true)
            {
                Console.WriteLine($"Created {result.Chunks.Count} chunks");
            }
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Extraction error: {ex.Message}");
        }
    }
}
