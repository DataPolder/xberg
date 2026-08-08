```csharp title="C#"
using Xberg;
using System;

var validator = new MinimumLengthValidator();
ValidatorRegistry.Register(validator);

public class MinimumLengthValidator : IValidator
{
    private const int MinimumLength = 10;

    public string Name => "min-length-validator";
    public string Version => "1.0.0";
    public int Priority => 50;

    public void Initialize()
    {
        Console.WriteLine($"Minimum length validator initialized (min: {MinimumLength})");
    }

    public void Shutdown()
    {
        Console.WriteLine("Minimum length validator shut down");
    }

    public void Validate(ExtractedDocument result, ExtractionConfig config)
    {
        if (result.Content.Length < MinimumLength)
        {
            throw new ValidationException(
                $"Content length {result.Content.Length} is below minimum {MinimumLength}"
            );
        }
    }

    public bool ShouldValidate(ExtractedDocument result, ExtractionConfig config)
    {
        return !string.IsNullOrEmpty(result.Content);
    }
}
```
