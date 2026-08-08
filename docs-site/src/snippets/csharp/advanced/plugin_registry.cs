using Xberg;
using System;
using System.Text.Json;

class Program
{
    static void Main()
    {
        try
        {
            var extractors = XbergConverter.ListDocumentExtractors();
            Console.WriteLine("Registered Document Extractors:");
            foreach (var extractor in extractors)
            {
                Console.WriteLine($"  - {extractor}");
            }

            var ocrBackends = XbergConverter.ListOcrBackends();
            Console.WriteLine("\nRegistered OCR Backends:");
            foreach (var backend in ocrBackends)
            {
                Console.WriteLine($"  - {backend}");
            }

            var processors = XbergConverter.ListPostProcessors();
            Console.WriteLine("\nRegistered Post-Processors:");
            foreach (var processor in processors)
            {
                Console.WriteLine($"  - {processor}");
            }

            var validators = XbergConverter.ListValidators();
            Console.WriteLine("\nRegistered Validators:");
            foreach (var validator in validators)
            {
                Console.WriteLine($"  - {validator}");
            }

            var customProcessor = new CustomPostProcessor();
            PostProcessorRegistry.RegisterPostProcessor(customProcessor);
            Console.WriteLine($"\nRegistered custom post-processor: {customProcessor.Name}");

            PostProcessorRegistry.Unregister(customProcessor.Name);
            Console.WriteLine($"Unregistered post-processor: {customProcessor.Name}");

            ValidatorRegistry.Clear();
            Console.WriteLine("All validators cleared");
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Plugin registry error: {ex.Message}");
        }
    }
}

class CustomPostProcessor : IPostProcessor
{
    public string Name => "custom-processor";
    public string Version => "1.0.0";
    public int Priority => 50;
    public ProcessingStage ProcessingStage => ProcessingStage.Late;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldProcess(ExtractedDocument result, ExtractionConfig config) => true;
    public ulong EstimatedDurationMs(ExtractedDocument result) => 1;

    // ExtractedDocument.Content is immutable; post-processors report derived
    // data through Metadata.Additional instead of rewriting the content.
    public void Process(ExtractedDocument result, ExtractionConfig config)
    {
        result.Metadata.Additional["content_uppercase"] =
            JsonSerializer.SerializeToElement(result.Content.ToUpper());
    }
}
