```typescript title="TypeScript"
import { describe, it, expect } from "vitest";
import {
  registerPostProcessor,
  registerValidator,
  unregisterPostProcessor,
  unregisterValidator,
  type PostProcessor,
  type Validator,
  type ExtractedDocument,
} from "@xberg-io/xberg";

describe("Plugin Testing", () => {
  describe("PostProcessor", () => {
    it("should add metadata to extraction result", async () => {
      const processor: PostProcessor = {
        name: () => "test-processor",
        priority: () => 10,
        async process(result?: ExtractedDocument | null): Promise<void> {
          if (!result) {
            return;
          }
          Object.assign(result, {
            metadata: {
              ...result.metadata,
              additional: {
                ...result.metadata?.additional,
                processed: true,
                processedAt: new Date().toISOString(),
              },
            },
          });
        },
      };

      registerPostProcessor(processor);

      const mockResult: ExtractedDocument = {
        content: "Test content",
        mimeType: "text/plain",
        metadata: { additional: { custom: "value" } },
        tables: [],
        detectedLanguages: [],
        chunks: undefined,
        images: undefined,
      };

      await processor.process(mockResult);

      expect(mockResult.metadata?.additional?.processed).toBe(true);
      expect(mockResult.metadata?.additional?.custom).toBe("value");

      unregisterPostProcessor("test-processor");
    });
  });

  describe("Validator", () => {
    it("should validate content length", async () => {
      const validator: Validator = {
        name: () => "length-validator",
        priority: () => 10,
        async validate(result?: ExtractedDocument | null): Promise<void> {
          if ((result?.content ?? "").length < 10) {
            throw new Error("Content too short");
          }
        },
      };

      registerValidator(validator);

      const mockResult: ExtractedDocument = {
        content: "Short",
        mimeType: "text/plain",
        metadata: {},
        tables: [],
        detectedLanguages: [],
        chunks: undefined,
        images: undefined,
      };

      await expect(validator.validate(mockResult)).rejects.toThrow("Content too short");

      unregisterValidator("length-validator");
    });

    it("should pass validation for valid content", async () => {
      const validator: Validator = {
        name: () => "length-validator-pass",
        priority: () => 10,
        async validate(result?: ExtractedDocument | null): Promise<void> {
          if ((result?.content ?? "").length < 10) {
            throw new Error("Content too short");
          }
        },
      };

      const mockResult: ExtractedDocument = {
        content: "This is a valid long content",
        mimeType: "text/plain",
        metadata: {},
        tables: [],
        detectedLanguages: [],
        chunks: undefined,
        images: undefined,
      };

      await expect(validator.validate(mockResult)).resolves.not.toThrow();
    });
  });
});
```
