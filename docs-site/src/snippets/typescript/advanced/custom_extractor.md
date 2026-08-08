```typescript title="TypeScript"
import {
  listDocumentExtractors,
  registerDocumentExtractor,
  unregisterDocumentExtractor,
  type DocumentExtractor,
  type ExtractedDocument,
} from "@xberg-io/xberg";

// List all registered document extractors
const extractors = listDocumentExtractors();
console.log("Available extractors:", extractors);
// Example output: ['PDFExtractor', 'ImageExtractor', 'OfficeExtractor', ...]

// Custom extractors ARE supported: implement DocumentExtractor and register it.
// Priority 60 outranks the built-in extractors (default priority 50).
const customExtractor: DocumentExtractor = {
  name: () => "custom-text-extractor",
  supportedMimeTypes: () => ["text/x-custom"],
  priority: () => 60,
  async extract(): Promise<ExtractedDocument> {
    return { content: "custom extraction result", mimeType: "text/x-custom" };
  },
};
registerDocumentExtractor(customExtractor);

// Unregister a registered extractor (built-in or custom) when no longer needed.
unregisterDocumentExtractor("custom-text-extractor");
```
