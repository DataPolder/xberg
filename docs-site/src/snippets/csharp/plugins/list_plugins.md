```csharp title="C#"
using Xberg;
using System;

var extractors = XbergConverter.ListDocumentExtractors();
Console.WriteLine("Registered extractors: " + string.Join(", ", extractors));

var ocrBackends = XbergConverter.ListOcrBackends();
Console.WriteLine("Registered OCR backends: " + string.Join(", ", ocrBackends));

var processors = XbergConverter.ListPostProcessors();
Console.WriteLine("Registered post-processors: " + string.Join(", ", processors));

var validators = XbergConverter.ListValidators();
Console.WriteLine("Registered validators: " + string.Join(", ", validators));
```
