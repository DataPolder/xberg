using Xberg;
using System;
using System.Threading.Tasks;

class MinLengthValidator : IValidator
{
    private readonly int _minLength;

    public MinLengthValidator(int minLength)
    {
        _minLength = minLength;
    }

    public string Name => "min-length";
    public string Version => "1.0.0";
    public int Priority => 10;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldValidate(ExtractedDocument result, ExtractionConfig config) => true;

    public void Validate(ExtractedDocument result, ExtractionConfig config)
    {
        if (result.Content.Length < _minLength)
        {
            throw new ValidationException(
                $"Content too short: {result.Content.Length} < {_minLength}"
            );
        }
    }
}

class QualityScoreValidator : IValidator
{
    private readonly double _minScore;

    public QualityScoreValidator(double minScore)
    {
        _minScore = minScore;
    }

    public string Name => "quality-score";
    public string Version => "1.0.0";
    public int Priority => 5;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldValidate(ExtractedDocument result, ExtractionConfig config) => result.QualityScore.HasValue;

    public void Validate(ExtractedDocument result, ExtractionConfig config)
    {
        var score = result.QualityScore ?? 0.0;

        if (score < _minScore)
        {
            throw new ValidationException(
                $"Quality score too low: {score:F2} < {_minScore:F2}"
            );
        }
    }
}

class ContentValidValidator : IValidator
{
    public string Name => "content-valid";
    public string Version => "1.0.0";
    public int Priority => 20;

    public void Initialize() { }
    public void Shutdown() { }

    public bool ShouldValidate(ExtractedDocument result, ExtractionConfig config) => true;

    public void Validate(ExtractedDocument result, ExtractionConfig config)
    {
        if (string.IsNullOrWhiteSpace(result.Content))
        {
            throw new ValidationException("Extracted content is empty or whitespace");
        }

        if (result.Content.Length < 10)
        {
            throw new ValidationException("Extracted content is too short (minimum 10 characters)");
        }
    }
}

class Program
{
    static async Task Main()
    {
        var minLengthValidator = new MinLengthValidator(minLength: 50);
        var qualityValidator = new QualityScoreValidator(minScore: 0.7);
        var contentValidator = new ContentValidValidator();

        ValidatorRegistry.RegisterValidator(minLengthValidator);
        ValidatorRegistry.RegisterValidator(qualityValidator);
        ValidatorRegistry.RegisterValidator(contentValidator);

        try
        {
            var config = new ExtractionConfig
            {
                EnableQualityProcessing = true
            };

            var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

            Console.WriteLine("All validations passed");
            Console.WriteLine($"Content length: {result.Content.Length}");
        }
        catch (ValidationException ex)
        {
            Console.WriteLine($"Validation failed: {ex.Message}");
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
