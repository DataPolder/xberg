```csharp title="C#"
using Xberg;
using System;

DocumentExtractorRegistry.Clear();
OcrBackendRegistry.Clear();
PostProcessorRegistry.Clear();
ValidatorRegistry.Clear();

Console.WriteLine("All plugins cleared");
```
