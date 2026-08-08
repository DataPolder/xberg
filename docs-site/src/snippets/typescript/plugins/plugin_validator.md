```ts title="TypeScript"
import {
  extract,
  registerValidator,
  unregisterValidator,
  ExtractInputKind,
  type Validator,
  type ExtractedDocument,
} from "@xberg-io/xberg";

class MinLengthValidator implements Validator {
  name(): string {
    return "min_length_validator";
  }

  priority(): number {
    return 10;
  }

  async validate(result?: ExtractedDocument | null): Promise<void> {
    const content = result?.content ?? "";
    if (content.length < 50) {
      throw new Error(`Content too short: ${content.length}`);
    }
  }
}

registerValidator(new MinLengthValidator());

const output = await extract({
  kind: ExtractInputKind.Uri,
  uri: "document.pdf",
});
const first = output.results?.[0];
console.log(`Validated content length: ${first?.content?.length ?? 0}`);

unregisterValidator("min_length_validator");
```
