```typescript title="TypeScript"
import {
  registerPostProcessor,
  ProcessingStage,
  type PostProcessor,
  type ExtractedDocument,
} from "@xberg-io/xberg";

class PdfOnlyProcessor implements PostProcessor {
  name(): string {
    return "pdf-only-processor";
  }

  processingStage(): ProcessingStage {
    return ProcessingStage.Middle;
  }

  // Gate the processor so it only runs for PDF documents.
  shouldProcess(result?: ExtractedDocument | null): boolean {
    return result?.mimeType === "application/pdf";
  }

  async process(): Promise<void> {
    // No-op: this processor only exists to demonstrate `shouldProcess` gating.
  }
}

registerPostProcessor(new PdfOnlyProcessor());
```
