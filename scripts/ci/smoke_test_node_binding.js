#!/usr/bin/env node
// Prove an installed xberg Node native binding (.node file) actually extracts
// documents, not just that require() resolves it.
//
// NAPI-RS's generated index.js runs the native addon's module-registration
// code synchronously at require() time (it is the require() call itself that
// dlopen()s the .node file), so a missing native library, a wrong-arch build,
// or an unresolvable shared-lib dependency (ONNX Runtime, libheif, ...) is
// already caught by requiring index.js. But that only proves the module
// loaded -- it does not prove any exported function actually runs correctly:
// a panic inside the extraction pipeline, or a subtly broken vendored shared
// library that resolves at dlopen time but misbehaves at call time, would
// still pass a load-only check. This script therefore runs a real extraction
// against a known fixture and asserts the exact expected text comes back.
//
// Usage: smoke_test_node_binding.js <path-to-index.js> <fixture-path> <expected-substring>
// Exit code is non-zero, with a message on stderr, on any failure: require()
// failure, extract() rejecting, an empty result, or a content mismatch.
"use strict";

const path = require("node:path");

function fail(message) {
  console.error(`SMOKE TEST FAILED: ${message}`);
  process.exitCode = 1;
}

async function run(indexPath, fixturePath, expectedSubstring) {
  let extract;
  try {
    ({ extract } = require(path.resolve(indexPath)));
  } catch (err) {
    fail(`could not load the xberg native binding via ${indexPath}: ${err}`);
    return;
  }

  if (typeof extract !== "function") {
    fail(`${indexPath} loaded but does not export an "extract" function`);
    return;
  }

  let result;
  try {
    result = await extract({ kind: "uri", uri: path.resolve(fixturePath) }, undefined);
  } catch (err) {
    fail(`extract() rejected for fixture ${fixturePath}: ${err}`);
    return;
  }

  if (!result || !Array.isArray(result.results) || result.results.length === 0) {
    fail(`extract() returned zero results for fixture ${fixturePath}`);
    return;
  }

  const content = result.results[0].content;
  if (typeof content !== "string" || !content.includes(expectedSubstring)) {
    fail(
      `expected substring ${JSON.stringify(expectedSubstring)} not found in extracted ` +
        `content for fixture ${fixturePath}; got ${JSON.stringify(content)}`,
    );
    return;
  }

  console.log(`SMOKE TEST PASSED: found ${JSON.stringify(expectedSubstring)} in extracted content`);
}

function main() {
  const [, , indexPath, fixturePath, expectedSubstring] = process.argv;
  if (!indexPath || !fixturePath || !expectedSubstring) {
    fail(`usage: ${process.argv[1]} <index.js-path> <fixture-path> <expected-substring>`);
    return;
  }
  run(indexPath, fixturePath, expectedSubstring).catch((err) => {
    fail(`unexpected error: ${err}`);
  });
}

main();
