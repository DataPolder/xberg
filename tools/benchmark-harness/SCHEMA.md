# Aggregation Schema v2.9.0

This document describes the structure of `aggregated.json` produced by `benchmark-harness consolidate`.

## Top-level Shape

```json
{
  "schema_version": "2.9.0",
  "by_framework_mode": {
    "<aggregate_key>": {
      /* FrameworkModeAggregation */
    }
  },
  "disk_sizes": {
    "framework": {
      /* DiskSizeInfo */
    }
  },
  "comparison": {
    /* ComparisonData */
  },
  "per_fixture_results": [
    /* PerFixtureRow[] */
  ],
  "metadata": {
    /* ConsolidationMetadata */
  },
  "run_provenance": [
    /* RunProvenanceRecord[] — v2.8.0+, see "Migration from v2.7.0 to v2.8.0" */
  ],
  "failure_summary": {
    /* FailureSummary — v2.9.0+, see "FailureSummary" */
  },
  "format_support": {
    /* FormatSupportMatrix — v2.9.0+, see "FormatSupportMatrix" */
  }
}
```

## Output Format Discriminator

The `output_format` field determines:

- **`markdown`**: Supports all metrics including SF1 (structural F1), layout percentiles, and all ranking tables
- **`plaintext`**: Text-only extraction; SF1 and layout percentiles are `null`; plaintext frameworks never appear in SF1 rankings

## by_framework_mode

Key format differs by framework family:

- **xberg** (`xberg-*`): `{framework_name}:{mode}` — the output format is already encoded
  in the framework name (e.g. `xberg-markdown-baseline`), so repeating it in the key is
  redundant.
- **competitors** (all other frameworks): `{framework}:{output_format}:{mode}` — format is not
  encoded in the name, so the key carries it explicitly.

Examples:

- `xberg-markdown-baseline:single`
- `xberg-plaintext-paddle-ocr:batch`
- `unstructured:plaintext:single`
- `docling:markdown:single`

Each entry contains:

```json
{
  "framework": "string", // Framework name without mode suffix
  "output_format": "markdown|plaintext", // Output format used
  "mode": "single|batch|...", // Execution mode
  "cold_start": {
    /* DurationPercentiles */
  }, // Optional, if cold start data available
  "by_file_type": {
    "pdf": {
      "file_type": "pdf",
      "no_ocr": {
        /* PerformancePercentiles */
      },
      "with_ocr": {
        /* PerformancePercentiles */
      }
    }
  }
}
```

## PerformancePercentiles

Contains p50, p95, p99 for all metrics:

```json
{
  "successful_sample_count": 42,
  "total_sample_count": 50,
  "framework_errors": 0,
  "harness_errors": 5,
  "timeouts": 3,
  "empty_content": 0,
  "error_details": {
    "error message": 2
  },
  "duration": { "p50": 100.5, "p95": 150.2, "p99": 199.9 },
  "throughput": { "p50": 5.2, "p95": 4.8, "p99": 3.1 },
  "memory": { "p50": 150.0, "p95": 200.0, "p99": 250.0 },
  "extraction_duration": { "p50": 80.0, "p95": 120.0, "p99": 160.0 }, // Optional
  "quality": {
    /* QualityPercentiles */
  }, // Optional, if quality data available
  "success_rate_percent": 84.0,
  "pages_per_sec": { "p50": 12.5, "p95": 8.0, "p99": 5.0 }, // Optional (v2.7.0+, PDF only)
  "cpu_seconds": { "p50": 1.2, "p95": 2.1, "p99": 3.4 }, // v2.7.0+
  "batch_size": 8, // Optional (v2.7.0+)
  "system_load": {
    /* SystemLoadPercentiles */
  }, // Optional (v2.7.0+)
  "throughput_excluded_sample_count": 0 // v2.8.0+
}
```

## QualityPercentiles

Includes p50, p95, p99 for all F1 metrics. Layout percentiles are `null` for plaintext-only frameworks:

```json
{
  "f1_text_p50": 0.92,
  "f1_text_p95": 0.88,
  "f1_text_p99": 0.75,
  "f1_numeric_p50": 0.85,
  "f1_numeric_p95": 0.8,
  "f1_numeric_p99": 0.7,
  "f1_layout_p50": 0.78, // null for plaintext output format
  "f1_layout_p95": 0.72, // null for plaintext output format
  "f1_layout_p99": 0.65, // null for plaintext output format
  "quality_score_p50": 0.85,
  "quality_score_p95": 0.8,
  "quality_score_p99": 0.7
}
```

## SystemLoadPercentiles (v2.7.0+)

A contention qualifier aggregated from `BenchmarkResult.system_load` snapshots in the group.
`null` when no result in the group carries a snapshot. See the "Tier A comparative performance
metrics" migration notes below for how to read `load_per_core`.

```json
{
  "load_per_core_p50": 0.35,
  "load_per_core_p95": 0.9,
  "contended_sample_count": 3,
  "total_sample_count": 20
}
```

## PerFixtureRow

One row per unique combination of (framework, output_format, execution_mode, fixture_id, ocr).
The batch dedup that collapses a native batch to one `performance_sample_count` in
`PerformancePercentiles` does **not** apply here: every per-document row from a batch is present
(see "Migration from v2.7.0 to v2.8.0").

```json
{
  "framework": "xberg-markdown-baseline",
  "output_format": "markdown",
  "execution_mode": "single",
  "ocr": false,
  "fixture_id": "sample_doc_1",
  "file_type": "pdf",
  "duration_ms": 125.4,
  "peak_memory_mb": 180.5,
  "f1_text": 0.92,
  "f1_layout": 0.78, // null for plaintext mode
  "f1_numeric": 0.85,
  "quality_score": 0.85,
  "correct": true,
  "success": true,
  "error_kind": null, // "FrameworkError", "HarnessError", "Timeout", etc. if !success
  "file_size": 45210, // v2.8.0+
  "throughput_bytes_per_sec": 361280.5, // v2.8.0+
  "avg_cpu_percent": 42.1, // v2.8.0+
  "cpu_seconds": 0.31, // v2.8.0+
  "baseline_memory_bytes": 12582912, // v2.8.0+
  "peak_memory_delta_bytes": 176160768, // v2.8.0+
  "p50_memory_bytes": 150000000, // v2.8.0+, this single measurement's own sampler percentile
  "p95_memory_bytes": 175000000, // v2.8.0+
  "p99_memory_bytes": 189000000, // v2.8.0+
  "extraction_duration_ms": 98.0, // Optional, v2.8.0+
  "subprocess_overhead_ms": 27.4, // Optional, v2.8.0+
  "cold_start_duration_ms": 210.0, // Optional, v2.8.0+
  "error_message": null, // Optional free-text error, v2.8.0+
  "quality": {
    /* QualityMetrics, including missing_tokens/extra_tokens — Optional, v2.8.0+ */
  },
  "pdf_metadata": {
    /* PdfMetadata — Optional, v2.8.0+ */
  },
  "framework_capabilities": {
    /* FrameworkCapabilities, including batch_capability — v2.8.0+ */
  },
  "system_load": {
    /* SystemLoad — Optional, v2.8.0+ */
  },
  "iterations": [
    /* IterationResult[] — v2.8.0+, empty unless multiple iterations were run */
  ],
  "statistics": {
    /* DurationStatistics — Optional, v2.8.0+ */
  }
}
```

`ocr` is `true` or `false` only when actual OCR usage is known. It is `null` when the framework did not report OCR usage.

The `duration_ms`/`peak_memory_mb`/`f1_*`/`quality_score`/`correct` fields are the original
v2.3.0 convenience projection and remain unchanged. The v2.8.0 fields above make each row
losslessly carry every measured field from its source `BenchmarkResult` — including ones with no
earlier scalar equivalent, like the free-text `error_message` or the token-level
`quality.missing_tokens`/`extra_tokens` lists.

## FailureSummary (v2.9.0+)

Top-level `failure_summary` rolls the per-framework-mode error counts (which also live on each
[`PerformancePercentiles`](#performancepercentiles)) up to the cohort level, preserving the
framework-fault vs infrastructure split throughout:

```json
{
  "total": { /* FailureCounts */ },
  "by_framework_mode": { "<aggregate_key>": { /* FailureCounts */ } },
  "by_file_type": { "docx": { /* FailureCounts */ } }
}
```

Each `FailureCounts` object:

```json
{
  "framework_errors": 0,     // framework hit a hard error on a supported document
  "empty_content": 0,        // framework produced empty output for a supported document
  "timeouts": 0,             // framework exceeded the timeout on a supported document
  "framework_fault_total": 0, // sum of the three above; these penalize quality + success rate
  "harness_errors": 0,       // our harness failed (process crash, bad JSON): never penalizes
  "config_setup_errors": 0,  // our config/setup failed (missing deps): never penalizes
  "infra_total": 0           // sum of the two infrastructure kinds above
}
```

Framework-fault failures are the framework's own fault — it was handed a document in a format it
declares support for and failed — and are scored as quality 0 in the percentiles. Infrastructure
failures are our harness's fault and are excluded from both the quality percentiles and the
success-rate denominator. `total` counts every document; `by_framework_mode` and `by_file_type`
partition the same failures by aggregate key and by file extension respectively.

## FormatSupportMatrix (v2.9.0+)

Top-level `format_support` distinguishes formats a framework declares unsupported from formats it
attempted and failed. It uses the same capability table that filters fixtures before execution.

```json
{
  "file_types": ["docx", "pdf", "rtf"],
  "unsupported": {
    "docling": ["rtf"],
    "liteparse": ["docx", "rtf"]
  }
}
```

`file_types` is the sorted set observed anywhere in the consolidated results. Release aggregates
include xberg's full-corpus run, so this is the complete corpus format set there. A competitor-only
aggregate cannot list a format that produced no result for any selected framework. `unsupported`
maps each logical framework to observed file types absent from its declared capabilities; xberg is
never listed because it is the full-corpus subject under test. Both fields default empty when older
v2.9.0 aggregates are deserialized.

## Migration from v2.7.0 to v2.8.0

Additive only — no key-format change, no field removed or renamed. Motivated by an aggregation
audit that found several measured fields from `results.json` (and the entire `provenance.json`
sidecar) were silently dropped during consolidation.

- **`run_provenance`** (`RunProvenanceRecord[]`, top-level, new): one entry per input directory
  the `consolidate` command read from, folding in the `provenance.json` sidecar that
  `load_run_results` previously ignored entirely. `aggregate_new_format` itself always leaves
  this empty (it has no filesystem access); the `consolidate` CLI command populates it. Shape:

  ```json
  {
    "source_dir": "xberg-markdown-baseline-batch",
    "provenance": {
      /* RunProvenance — see "Run provenance sidecar" below — or null */
    },
    "missing_reason": "provenance.json not found in ..." // Present only when provenance is null
  }
  ```

  A missing `provenance.json` is recorded, not treated as an error — see
  `consolidate::load_run_provenance`.
- **`PerFixtureRow`**: extended with `file_size`, `throughput_bytes_per_sec`, `avg_cpu_percent`,
  `cpu_seconds`, `baseline_memory_bytes`, `peak_memory_delta_bytes`, `p50/p95/p99_memory_bytes`,
  `extraction_duration_ms`, `subprocess_overhead_ms`, `cold_start_duration_ms`, `error_message`,
  `quality` (the full `QualityMetrics`, including `missing_tokens`/`extra_tokens`),
  `pdf_metadata`, `framework_capabilities` (including `batch_capability`), `system_load`,
  `iterations`, and `statistics`. See the `PerFixtureRow` section above for the full shape.
- **`PerformancePercentiles.throughput_excluded_sample_count`** (`usize`, new): the number of
  successful performance samples excluded from the `throughput` percentiles because their
  throughput was zero, negative, or non-finite. The exclusion rule itself is unchanged from
  pre-v2.8.0 behavior; this field only makes a previously-silent exclusion visible.
- **`ConsolidationMetadata.disk_size_conflicts`** (`string[]`, new, defaults to `[]`): human-
  readable notes for any framework where two or more results reported a different
  `installation_size`. `disk_sizes` has always kept only the last-seen value per framework; this
  field surfaces when that last-writer-wins behavior actually discarded a conflicting value.

## Migration from v2.6.0 to v2.7.0

- The published SF1 (`f1_layout`) now folds in the D6 table cell-content dimension
  (GriTS-Con), weighted `1.5` — equal to table topology. A table with the right grid
  but wrong cell text now scores lower than one with correct content. D6 was already
  computed and reported in prior versions; it is now part of the rollup, so SF1 values
  and PDF rankings shift downward for frameworks that reconstruct grids but garble cells.
  Table-less documents are unaffected (D6 is gated on table presence).

### Tier A comparative performance metrics (additive)

All fields below are additive to the `2.7.0` schema (no version bump, no key-format change).
Consumers built against the fields documented in "Migration from v2.5.0" and earlier remain
compatible: every new field is either optional (`null`/absent when not applicable) or has a
type-appropriate default (`0.0` for numeric percentiles) when deserializing older artifacts that
predate it.

- **`PerformancePercentiles.pages_per_sec`** (optional `Percentiles`): pages-per-second
  percentiles, derived from the harness-side (framework-agnostic) `PdfMetadata.page_count`
  divided by wall-clock duration — never a framework's self-reported page count, so every
  framework is compared against the same ground truth. `null` when no result in the group
  carries a known PDF page count (non-PDF file types, or a PDF the harness could not size). For
  a native batch, the page counts of every document sharing one batch invocation are summed
  before dividing by the batch's shared makespan, mirroring how `throughput` is computed for
  batches from summed bytes rather than a single member row.
- **`PerformancePercentiles.cpu_seconds`** (`Percentiles`, always present): total process-tree
  CPU-time percentiles, in core-seconds. Computed by trapezoidal integration of the resource
  sampler's per-sample CPU percentage over its timeline (see
  `PerformanceMetrics.cpu_seconds` doc comment). **Precision is bounded by the sampling
  interval** (1-10ms, adaptive on file size): CPU bursts shorter than the gap between two
  samples are smoothed by the trapezoidal average rather than measured exactly, so treat
  `cpu_seconds` as an approximation, not an exact accounting figure.
- **`PerformancePercentiles.batch_size`** (optional `usize`): the approximate number of
  documents processed per one measured process invocation in this group — `1` for single-file
  mode, or the modal per-batch document count for a native batch (`total_sample_count /
  performance_sample_count`, rounded). This lets peak-RSS (and any other performance metric
  already in this struct) be read "keyed by batch size" using the single-file-vs-batch mode
  entries the benchmark matrix already runs, without adding a new axis to the
  `by_framework_mode` aggregate key.
- **`PerformancePercentiles.system_load`** (optional `SystemLoadPercentiles`): surfaces the
  previously-captured-but-discarded `BenchmarkResult.system_load` as a contention qualifier —
  `null` when no result in the group carries a snapshot. Read `load_per_core` *relatively* (was
  this bucket measured under comparable contention to another) rather than as an absolute
  number; `contended_sample_count` counts samples whose 1-minute load average per logical core
  exceeded the harness's contention threshold.
- **`ComparisonData.pages_per_sec_ranking`** / **`cpu_seconds_ranking`** (`RankedFramework[]`):
  new rankings mirroring `throughput_ranking` and `memory_ranking` respectively — pages/sec
  ranked descending (higher is better), CPU-seconds ranked ascending (lower is better). Only
  frameworks with at least one pages/sec observation appear in `pages_per_sec_ranking`.
- **`ComparisonData.pareto_frontier`** (`ParetoPoint[]`): the non-dominated frontier over
  (pages/sec ↑, SF1 ↑, peak-RSS ↓) for markdown frameworks that carry both an SF1 term and a
  pages/sec observation (plaintext-only frameworks never carry SF1, so they are never eligible;
  see `ParetoPoint`'s doc comment for the dominance rule). Pure computation over already-reported
  percentiles — no new capture-time data required.

## Migration from v2.5.0 to v2.6.0

- `PerFixtureRow.ocr` changed from a required boolean to a nullable boolean so unknown OCR usage is not mislabeled as `false`.

## ComparisonData

Contains all cross-framework rankings split by output format for quality metrics:

```json
{
  "throughput_ranking": [
    /* RankedFramework[] */
  ],
  "memory_ranking": [
    /* RankedFramework[] */
  ],
  "quality_ranking_markdown": [
    /* RankedFramework[] — markdown-only (combined quality with SF1 term) */
  ],
  "quality_ranking_plaintext": [
    /* RankedFramework[] — plaintext-only (combined quality, no SF1 term) */
  ],
  "pdf_quality_ranking_markdown": [
    /* RankedFramework[] — markdown-only, never plaintext */
  ],
  "pdf_quality_ranking_plaintext": [
    /* RankedFramework[] — plaintext-only */
  ],
  "pdf_tf1_ranking_markdown": [
    /* RankedFramework[] — markdown-only */
  ],
  "pdf_tf1_ranking_plaintext": [
    /* RankedFramework[] — plaintext-only */
  ],
  "pdf_sf1_ranking_markdown": [
    /* RankedFramework[] — markdown-only, never plaintext */
  ],
  "pages_per_sec_ranking": [
    /* RankedFramework[] — descending, higher pages/sec first (v2.7.0+) */
  ],
  "cpu_seconds_ranking": [
    /* RankedFramework[] — ascending, lower CPU-seconds first (v2.7.0+) */
  ],
  "deltas_vs_baseline": {
    "<aggregate_key>": {
      /* DeltaMetrics */
    }
  },
  "pareto_frontier": [
    /* ParetoPoint[] — non-dominated (pages/sec, SF1, peak-RSS) points, markdown only (v2.7.0+) */
  ]
}
```

### RankedFramework

```json
{
  "framework_mode": "xberg-markdown-baseline:single",
  "rank": 1,
  "value": 95.5, // The metric value (duration, throughput, etc.)
  "relative": 1.0 // Ratio relative to best (1.0 = best)
}
```

### ParetoPoint (v2.7.0+)

One non-dominated point in the (pages/sec, SF1, peak-RSS) multi-objective comparison. A
candidate is on the frontier when no other candidate dominates it: dominance requires being at
least as good on every objective and strictly better on at least one. `pages_per_sec` and `sf1`
are maximized; `peak_memory_mb` is minimized.

```json
{
  "framework_mode": "xberg-markdown-layout:single",
  "pages_per_sec": 12.5,
  "sf1": 0.82,
  "peak_memory_mb": 320.0
}
```

## Migration from v2.3.0 to v2.4.0

### Breaking Changes

1. **Schema version**: Bumped to `"2.4.0"`
2. **Xberg aggregate key format**: Changed from `framework:output_format:mode` to
   `framework_name:mode` for all `xberg-*` frameworks. Competitor key format
   (`framework:output_format:mode`) is unchanged.

### Xberg Consolidation

Language-binding frameworks (`xberg-py`, `xberg-node`, `xberg-rb`, `xberg-go`,
`xberg-java`, `xberg-csharp`, `xberg-elixir`, `xberg-php`, `xberg-rust`, etc.)
have been removed. They are replaced by three native pipelines run directly via the xberg CLI:

| Pipeline  | Markdown name                   | Plaintext name                   |
| --------- | ------------------------------- | -------------------------------- |
| Baseline  | `xberg-markdown-baseline`   | `xberg-plaintext-baseline`   |
| Layout    | `xberg-markdown-layout`     | `xberg-plaintext-layout`     |
| PaddleOCR | `xberg-markdown-paddle-ocr` | `xberg-plaintext-paddle-ocr` |

Batch variants append `-batch` to the framework name (e.g. `xberg-markdown-baseline-batch`),
which the harness normalises to aggregate key `xberg-markdown-baseline:batch`.

### Run provenance sidecar

The `run` command writes `provenance.json` beside the backward-compatible `results.json` array.
Schema version 2 records the Xberg repository commit/dirty bit, the ordered fixture descriptors
and document BLAKE3 digests, cohort manifest identity, adapter versions and executable digests,
explicit model revision identities, timing configuration, fixed batch partitions, requested
workers, framework-specific worker semantics, and the configured Xberg thread budget.
Local absolute paths are never serialized.

Since v2.8.0, the `consolidate` command reads every `provenance.json` sidecar it finds alongside
a `results.json` (via `consolidate::load_run_provenance`) and folds it into the aggregate's
top-level `run_provenance` array — see "Migration from v2.7.0 to v2.8.0" above. A directory with
`results.json` but no `provenance.json` is recorded with `provenance: null`, not treated as an
error.

For Xberg rows, `frameworks[].configured_thread_budget` records an explicit
`--xberg-max-threads` value in either mode. For native batch rows without an explicit
value, it records the legacy `--max-concurrent` fallback passed to `xberg batch
--max-threads`. It is distinct from
`frameworks[].requested_workers`, which records the `--max-concurrent` document-concurrency
cap. `effective_workers` remains `null` because Xberg resolves effective document concurrency
from the workload. Non-Xberg rows and automatic-budget single-file rows omit
`configured_thread_budget`.

#### Run provenance migration from schema 1 to schema 2

- Added optional `FrameworkProvenance.configured_thread_budget`.
- Existing schema-1 sidecars remain readable; a missing field deserializes as `null`.
- Consumers should treat `null` as unavailable, not infer a thread budget from worker fields.
- Native-batch producers that omit `--xberg-max-threads` record the legacy fallback
  value from `--max-concurrent`; single-file producers omit the field.

### Key Format Rationale

The format component is implicit in the xberg framework name itself. Duplicating it in the
aggregate key (`xberg-markdown-baseline:markdown:single`) would be redundant and confusing.
Competitor names carry no format information, so they continue to need it in the key
(`docling:markdown:single`).

## Migration from v2.2.0 to v2.3.0

### Breaking Changes

1. **Schema version**: Bumped to `"2.3.0"`
2. **Framework key format**: Changed from `framework:mode` to `framework:output_format:mode`
3. **QualityPercentiles**: Added p95 and p99 percentiles for all F1 metrics; `f1_layout_*` fields are now optional (null for plaintext)
4. **FrameworkModeAggregation**: Added `output_format` field
5. **ComparisonData**: Replaced `pdf_tf1_ranking` with `pdf_tf1_ranking_markdown` and `pdf_tf1_ranking_plaintext`; `pdf_sf1_ranking` renamed to `pdf_sf1_ranking_markdown` (now markdown-only)

### New Fields

- `per_fixture_results`: Array of individual fixture results preserving per-file measurements
- `PerFixtureRow`: New struct capturing individual extraction outcomes

### Plaintext-only Behavior

- Plaintext frameworks NEVER appear in `pdf_sf1_ranking_markdown`
- Plaintext frameworks NEVER appear in `pdf_tf1_ranking_markdown` (they get their own `pdf_tf1_ranking_plaintext`)
- SF1 and layout percentiles are `null` for plaintext output format
- All performance rankings (speed, memory, throughput) include both formats without discrimination

## ConsolidationMetadata

```json
{
  "total_results": 500,
  "framework_count": 5,
  "file_type_count": 8,
  "shared_corpus_markdown": ["pdf"],
  "shared_corpus_plaintext": ["pdf"],
  "timestamp": "2025-05-09T10:15:30Z",
  "disk_size_conflicts": [] // v2.8.0+, see "Migration from v2.7.0 to v2.8.0"
}
```

- **`framework_count`** counts *logical* frameworks: all `xberg-*` pipeline variants collapse to a
  single `xberg` before counting (so 7 competitors + xberg = 8, not 11).
- **`shared_corpus_markdown` / `shared_corpus_plaintext`** are the file types the "overall"
  `quality_ranking_markdown` / `quality_ranking_plaintext` are actually computed over — the
  intersection of file types every candidate framework of that format attempted. When a
  single-format framework (e.g. PDF-only `liteparse`/`mineru`) is in the pool, this collapses to
  that one type (e.g. `["pdf"]`), and the "overall" ranking must be read as that-type-only rather
  than a true all-format comparison.
