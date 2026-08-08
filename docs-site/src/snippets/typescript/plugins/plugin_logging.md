```typescript title="TypeScript"
import {
  registerPostProcessor,
  registerValidator,
  type PostProcessor,
  type Validator,
  type ExtractedDocument,
} from "@xberg-io/xberg";

class LoggingPostProcessor implements PostProcessor {
  name(): string {
    return "logging-processor";
  }

  priority(): number {
    return 5;
  }

  async process(result?: ExtractedDocument | null): Promise<void> {
    if (!result) {
      return;
    }
    const content = result.content ?? "";
    console.info(`[PostProcessor] Processing ${result.mimeType}`);
    console.info(`[PostProcessor] Content length: ${content.length}`);

    if (content.length === 0) {
      console.warn("[PostProcessor] Warning: Empty content extracted");
    }
  }
}

class LoggingValidator implements Validator {
  name(): string {
    return "logging-validator";
  }

  priority(): number {
    return 100;
  }

  async validate(result?: ExtractedDocument | null): Promise<void> {
    const content = result?.content ?? "";
    console.info(`[Validator] Validating extraction result (${content.length} bytes)`);

    if (content.length < 50) {
      console.error("[Validator] Error: Content below minimum threshold");
      throw new Error("Content too short");
    }
  }
}

// Register plugins with logging
registerPostProcessor(new LoggingPostProcessor());
registerValidator(new LoggingValidator());

console.log("[Main] Plugins registered with logging enabled");
```
