/* oxlint-disable */
import { extract, type ExtractionConfig } from "@xberg-io/xberg";

const config: ExtractionConfig = { useCache: true };

(async () => {
  console.log("First extraction (will be cached)...");
  const output1 = await extract({ kind: "uri", uri: "document.pdf" }, config);
  const result1 = output1.results[0];
  const length1 = result1.content.length;
  console.log("  - Content length: " + length1);

  console.log("\nSecond extraction (from cache)...");
  const output2 = await extract({ kind: "uri", uri: "document.pdf" }, config);
  const result2 = output2.results[0];
  const length2 = result2.content.length;
  console.log("  - Content length: " + length2);

  const isIdentical = result1.content === result2.content;
  console.log("\nResults are identical: " + isIdentical);
})();
