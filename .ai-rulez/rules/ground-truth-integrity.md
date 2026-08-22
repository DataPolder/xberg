---
priority: high
---

- Never use Xberg's own extractor output as benchmark ground truth. GT must come from an
  independent source, recorded in the fixture's `ground_truth.source` field
  (`manual`, `vision`, `pdf_text_layer`, `pandoc`, `python-docx`, …).
- Before blaming a ground-truth file for a scoring defect, verify the SOURCE document renders
  what the GT claims. A derived `.md`/`.txt` that disagrees with the rendered page is a real
  GT bug; one that agrees is a bug in the extractor or the metric.
- GT generation and validation live in `tools/benchmark-harness/`: fixture schema in its
  `README.md`, generation in `scripts/generate_markdown_gt.py`, integrity and HTML-to-GFM
  cleanup in `src/validate_gt.rs` (`validate-gt`). Use those — do not hand-roll a pandoc or
  `sed` pipeline.
