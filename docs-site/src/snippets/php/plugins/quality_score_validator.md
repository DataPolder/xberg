```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\Xberg;
use Xberg\Validator;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

class QualityScoreValidator implements Validator {
    private float $minQualityScore = 0.7;

    public function name(): string {
        return "quality-score-validator";
    }

    public function version(): string {
        return "1.0.0";
    }

    public function initialize(): void {
        // Load quality scoring models or rules
    }

    public function shutdown(): void {
        // Cleanup resources
    }

    public function validate(ExtractedDocument $result, ExtractionConfig $config): mixed {
        $qualityScore = $this->calculateQualityScore($result);

        if ($qualityScore < $this->minQualityScore) {
            throw new Exception(
                sprintf(
                    "Quality score too low: %.2f < %.2f",
                    $qualityScore,
                    $this->minQualityScore
                )
            );
        }

        return null;
    }

    public function priority(): int {
        return 90;
    }

    private function calculateQualityScore(ExtractedDocument $result): float {
        $score = 1.0;

        // Penalize if content is too short
        if (strlen($result->content) < 100) {
            $score *= 0.8;
        }

        // Penalize if many detection warnings
        if (count($result->getProcessingWarnings()) > 5) {
            $score *= 0.9;
        }

        // Reward if language was detected
        if (!empty($result->detectedLanguages)) {
            $score *= 1.05;
        }

        return min(1.0, $score);
    }
}

// Register the quality score validator
$validator = new QualityScoreValidator();
Xberg::registerValidator($validator);

echo "Quality score validator registered\n";
```
