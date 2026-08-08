```python title="Python"
import asyncio
from xberg import (
    ExtractInput,
    ExtractionConfig,
    ExtractedDocument,
    ValidationError,
    extract,
    register_validator,
)

class MinLengthValidator:
    def name(self) -> str:
        return "min_length"

    def version(self) -> str:
        return "1.0.0"

    def validate(self, result: ExtractedDocument, config: ExtractionConfig) -> None:
        if len(result.content) < 50:
            raise ValidationError(f"Content too short: {len(result.content)}")

    def should_validate(self, result: ExtractedDocument, config: ExtractionConfig) -> bool:
        return True

    def initialize(self) -> None:
        pass

    def shutdown(self) -> None:
        pass

validator: MinLengthValidator = MinLengthValidator()
register_validator(validator)

async def main() -> None:
    result = await extract(ExtractInput(uri="document.pdf"), ExtractionConfig())
    print(f"Content length: {len(result.results[0].content)}")

asyncio.run(main())
```
