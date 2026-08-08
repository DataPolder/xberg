```typescript title="TypeScript"
import { readFile } from "node:fs/promises";
import {
  registerDocumentExtractor,
  unregisterDocumentExtractor,
  type DocumentExtractor,
  type ExtractInput,
  type ExtractedDocument,
} from "@xberg-io/xberg";

/** Flattens every string leaf of a JSON value into plain text. */
function flatten(value: unknown): string {
  if (typeof value === "string") {
    return `${value}\n`;
  }
  if (Array.isArray(value)) {
    return value.map(flatten).join("");
  }
  if (typeof value === "object" && value !== null) {
    return Object.values(value).map(flatten).join("");
  }
  return "";
}

const jsonExtractor: DocumentExtractor = {
  name: () => "custom-json-extractor",
  version: () => "1.0.0",

  supportedMimeTypes: () => ["application/json", "text/json"],

  // Priority 60 outranks the built-in extractors (50) for those MIME types.
  priority: () => 60,

  async extract(input?: ExtractInput | null): Promise<ExtractedDocument> {
    // `kind: "bytes"` inputs carry the payload; `kind: "uri"` inputs carry a
    // path the plugin reads itself.
    const payload = input?.bytes ?? (input?.uri ? await readFile(input.uri) : undefined);
    if (payload === undefined) {
      throw new Error("custom-json-extractor: input has neither bytes nor uri");
    }

    let decoded: unknown;
    try {
      decoded = JSON.parse(Buffer.from(payload).toString("utf8"));
    } catch (cause) {
      throw new Error(`custom-json-extractor: parsing JSON failed`, { cause });
    }

    return { content: flatten(decoded), mimeType: "application/json" };
  },
};

registerDocumentExtractor(jsonExtractor);

// Remove it again when the plugin is no longer needed.
unregisterDocumentExtractor("custom-json-extractor");
```
