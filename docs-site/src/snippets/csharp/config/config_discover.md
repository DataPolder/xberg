```csharp title="C#"
using Xberg;

// NOTE: The C# binding has no config-file auto-discovery API
// (no ExtractionConfig.Discover() / xberg.toml lookup). Build the config
// programmatically or deserialize it yourself with ExtractionConfig.FromJson.
var config = ExtractionConfig.Default();

var result = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];
Console.WriteLine(result.Content);
```
