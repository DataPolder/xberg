using Xberg;
using System;

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

class Program
{
    static async System.Threading.Tasks.Task Main()
    {
        var minLengthValidator = new MinLengthValidator(minLength: 50);
        var qualityValidator = new QualityScoreValidator(minScore: 0.7);

        ValidatorRegistry.RegisterValidator(minLengthValidator);
        ValidatorRegistry.RegisterValidator(qualityValidator);

        try
        {
            var config = new ExtractionConfig
            {
                EnableQualityProcessing = true
            };

            var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];

            Console.WriteLine("Validation passed");
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
