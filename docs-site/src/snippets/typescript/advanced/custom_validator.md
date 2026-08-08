```typescript title="TypeScript"
import {
  registerValidator,
  unregisterValidator,
  extract,
  ExtractInputKind,
  type Validator,
  type ExtractedDocument,
} from "@xberg-io/xberg";

/**
 * Custom validator for quality checking
 * Ensures extracted content meets minimum quality standards
 * @example
 * const validator = new QualityValidator();
 * registerValidator(validator);
 */
class QualityValidator implements Validator {
  name(): string {
    return "quality-validator";
  }

  priority(): number {
    return 10;
  }

  /**
   * Validate extraction result meets quality standards
   */
  async validate(result?: ExtractedDocument | null): Promise<void> {
    if (!result) {
      return;
    }
    this.checkMinimumLength(result);
    this.checkEmptyContent(result);
    this.checkMetadata(result);
  }

  /**
   * Ensure minimum content length
   */
  private checkMinimumLength(result: ExtractedDocument): void {
    const minLength = 50;
    const content = result.content ?? "";
    if (content.length < minLength) {
      throw new Error(`Content too short: ${content.length} bytes (minimum ${minLength})`);
    }
  }

  /**
   * Ensure content is not empty
   */
  private checkEmptyContent(result: ExtractedDocument): void {
    const trimmed = (result.content ?? "").trim();
    if (trimmed.length === 0) {
      throw new Error("Extracted content is empty");
    }
  }

  /**
   * Validate metadata is present
   */
  private checkMetadata(result: ExtractedDocument): void {
    if (!result.metadata || Object.keys(result.metadata).length === 0) {
      throw new Error("Missing extraction metadata");
    }
  }
}

// Register the validator
const validator = new QualityValidator();
registerValidator(validator);

// Usage with error handling (must use async extraction for custom validators)
try {
  const output = await extract({
    kind: ExtractInputKind.Uri,
    uri: "document.pdf",
  });
  const first = output.results?.[0];
  console.log(`Validated content length: ${first?.content?.length ?? 0} characters`);
} catch (error) {
  console.error(`Validation failed: ${error instanceof Error ? error.message : String(error)}`);
}

// Later, unregister if needed
// unregisterValidator("quality-validator");
```
