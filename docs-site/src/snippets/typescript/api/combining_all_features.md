```typescript title="TypeScript"
import { extract } from "@xberg-io/xberg";

const config = {
  // OCR: Tesseract on all pages with English text
  forceOcr: false,
  ocr: {
    backend: "tesseract",
    language: ["eng"],
  },
  // Chunking: semantic markdown chunks of ~800 chars, 100-char overlap
  chunking: {
    maxCharacters: 800,
    overlap: 100,
    chunkerType: "markdown",
    prependHeadingContext: true,
  },
  // Output: include document structure and tables
  outputFormat: "markdown",
  includeDocumentStructure: true,
  // Images: extract embedded images
  images: {
    extractImages: true,
  },
  // Cache extracted results on disk
  useCache: true,
  enableQualityProcessing: true,
};

const output = await extract({ kind: "uri", uri: "report.pdf" }, config);
const result = output.results[0];

console.log(`Content (${result.content.length} chars):`);
console.log(result.content.slice(0, 200));

if (result.chunks) {
  console.log(`\nChunks: ${result.chunks.length}`);
}
console.log(`Tables: ${result.tables?.length ?? 0}`);
if (result.detectedLanguages) {
  console.log(`Languages: ${result.detectedLanguages}`);
}
if (result.extractionMethod) {
  console.log(`Extraction method: ${result.extractionMethod}`);
}
```
