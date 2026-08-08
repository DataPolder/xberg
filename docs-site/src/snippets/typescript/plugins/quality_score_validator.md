```typescript title="TypeScript"
import { registerValidator, type Validator, type ExtractedDocument } from "@xberg-io/xberg";

class QualityScoreValidator implements Validator {
  private readonly minScore: number;

  constructor(minScore: number = 0.5) {
    this.minScore = minScore;
  }

  name(): string {
    return "quality-score-validator";
  }

  priority(): number {
    return 50;
  }

  async validate(result?: ExtractedDocument | null): Promise<void> {
    if (!result) {
      return;
    }
    const score = Number(result.metadata?.additional?.quality_score ?? 0);

    if (score < this.minScore) {
      throw new Error(
        `Quality score too low: ${score.toFixed(2)} < ${this.minScore.toFixed(2)}`,
      );
    }
  }
}

registerValidator(new QualityScoreValidator(0.5));
```
