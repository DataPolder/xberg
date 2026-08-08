```typescript title="TypeScript"
import {
  registerPostProcessor,
  ProcessingStage,
  type PostProcessor,
  type ExtractedDocument,
} from "@xberg-io/xberg";

class PdfMetadataExtractor implements PostProcessor {
  private processedCount: number = 0;

  name(): string {
    return "pdf-metadata-extractor";
  }

  processingStage(): ProcessingStage {
    return ProcessingStage.Early;
  }

  shouldProcess(result?: ExtractedDocument | null): boolean {
    return result?.mimeType === "application/pdf";
  }

  async process(result?: ExtractedDocument | null): Promise<void> {
    if (!result) {
      return;
    }
    this.processedCount += 1;

    Object.assign(result, {
      metadata: {
        ...result.metadata,
        additional: {
          ...result.metadata?.additional,
          pdfProcessingIndex: this.processedCount,
          pdfMetadataEnriched: true,
        },
      },
    });
  }

  getStats(): { processedCount: number } {
    return { processedCount: this.processedCount };
  }
}

registerPostProcessor(new PdfMetadataExtractor());
```
