---
priority: high
---

- hOCR parsing: extract word-level bounding boxes, confidence scores, and text content
- Preserve spatial relationships from hOCR output for layout reconstruction
- Table detection is word-bbox clustering, NOT line detection or intersection analysis. `table_core::detect_rows` groups by y-centre against `row_threshold_ratio × median word height`; `detect_columns` groups by left edge within `column_threshold`.
- Merge words into cell tokens BEFORE detecting columns — `reconstruct_table_with_columns` calls `merge_words_into_cell_tokens(words, &row_positions)` and feeds the result to `detect_columns`. Multi-word cells otherwise mint one spurious column per word and the grid validator rejects every row (measured 0 -> 24 detected rows once merging landed).
- A caller that runs `detect_columns` on the raw words and then indexes a `reconstruct_table` grid by those positions is wrong — use `reconstruct_table_with_columns` and its returned positions.
- Validate grid structure before treating detected regions as tables
- Cells are never re-OCR'd. `reconstruct_table` assigns the ORIGINAL words to cells; the merged tokens exist only to position column boundaries.
- Convert tables to markdown format with proper column alignment
