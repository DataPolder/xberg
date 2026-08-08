```typescript title="TypeScript"
import { registerValidator, type Validator, type ExtractedDocument } from "@xberg-io/xberg";

class MinLengthValidator implements Validator {
  private readonly minLength: number;

  constructor(minLength: number) {
    this.minLength = minLength;
  }

  name(): string {
    return "min-length-validator";
  }

  priority(): number {
    return 100;
  }

  async validate(result?: ExtractedDocument | null): Promise<void> {
    const length = result?.content?.length ?? 0;
    if (length < this.minLength) {
      throw new Error(`Content too short: ${length} < ${this.minLength} characters`);
    }
  }
}

registerValidator(new MinLengthValidator(50));
```
