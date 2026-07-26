//! Aggregation module for benchmark results (v2.6.0 output schema).
//!
//! Groups [`BenchmarkResult`] records by framework-and-mode, output format, file type, and
//! OCR usage (yes/no), then computes percentile-based statistics for each
//! group. The output schema (`schema_version: "2.6.0"`) surfaces TF1 and SF1 separately
//! with per-fixture rows preserved and split rankings by output format.
//!
//! # Percentile methodology
//!
//! All percentiles use the **R-7 interpolation** method (the default in R and
//! NumPy) via [`crate::stats::percentile_r7`]. Three percentiles are reported
//! per metric: **p50** (median), **p95**, and **p99**. Values that are `NaN`
//! or `Inf` after interpolation are sanitized to `0.0` by
//! [`crate::stats::sanitize_f64`] so that downstream JSON consumers never
//! encounter non-finite floats.
//!
//! Failed results (non-zero `error_kind`) are excluded from percentile
//! calculations but still counted in `total_sample_count` to preserve the
//! `success_rate_percent` metric.
//!
//! # Output format support
//!
//! Plaintext-only frameworks must NEVER appear in SF1 rankings or quality metrics
//! that require layout information. Markdown frameworks appear in all rankings.
//!
//! # Aggregate key format
//!
//! Keys in `by_framework_mode` differ by framework family:
//!
//! - **xberg** (`xberg-*`): `{framework_name}:{mode}` — the output format is already
//!   encoded in the framework name (e.g. `xberg-markdown-baseline`), so repeating it in
//!   the key would be redundant.
//! - **competitors** (all other frameworks): `{framework}:{output_format}:{mode}` — format is
//!   not encoded in the name, so the key must carry it explicitly.

use crate::stats::{percentile_r7, sanitize_f64};
use crate::types::{BenchmarkResult, DiskSizeInfo, ErrorKind, OutputFormat, successful_performance_samples};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema version for the aggregated output format.
pub const SCHEMA_VERSION: &str = "2.7.0";

/// Consolidated results using aggregation format v2.6.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewConsolidatedResults {
    /// Schema version for this output format
    pub schema_version: String,
    /// Aggregated results grouped by framework:output_format:mode combination
    pub by_framework_mode: HashMap<String, FrameworkModeAggregation>,
    /// Disk sizes for each framework
    pub disk_sizes: HashMap<String, DiskSizeInfo>,
    /// Cross-framework comparison rankings
    pub comparison: ComparisonData,
    /// Per-fixture results (one row per framework:output_format:execution_mode:fixture_id:ocr)
    pub per_fixture_results: Vec<PerFixtureRow>,
    /// Metadata about the consolidation
    pub metadata: ConsolidationMetadata,
}

/// Per-fixture benchmark result row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerFixtureRow {
    /// Framework name
    pub framework: String,
    /// Output format (markdown or plaintext)
    pub output_format: OutputFormat,
    /// Execution mode (single, batch, etc.)
    pub execution_mode: String,
    /// Whether OCR was actually used, or `null` when the framework did not report it.
    pub ocr: Option<bool>,
    /// Fixture ID (e.g., from file path)
    pub fixture_id: String,
    /// File type/extension
    pub file_type: String,
    /// Total duration in milliseconds
    pub duration_ms: f64,
    /// Peak memory usage in MB
    pub peak_memory_mb: f64,
    /// Text F1 score (optional)
    pub f1_text: Option<f64>,
    /// Layout F1 score (optional, only for markdown mode)
    pub f1_layout: Option<f64>,
    /// Numeric F1 score (optional)
    pub f1_numeric: Option<f64>,
    /// Overall quality score (optional)
    pub quality_score: Option<f64>,
    /// Whether extraction was correct (optional)
    pub correct: Option<bool>,
    /// Whether extraction succeeded
    pub success: bool,
    /// Error kind if failed (optional)
    pub error_kind: Option<String>,
}

/// Cross-framework comparison rankings and deltas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonData {
    /// Frameworks ranked by median throughput (highest first)
    pub throughput_ranking: Vec<RankedFramework>,
    /// Frameworks ranked by median memory usage (lowest first)
    pub memory_ranking: Vec<RankedFramework>,
    /// Frameworks ranked by quality score (highest first) — markdown only. Plaintext-only
    /// frameworks are never scored against layout-inclusive quality, so they are excluded
    /// here (see module-level docs).
    pub quality_ranking_markdown: Vec<RankedFramework>,
    /// Frameworks ranked by quality score (highest first) — plaintext only.
    pub quality_ranking_plaintext: Vec<RankedFramework>,
    /// PDF-only: frameworks ranked by overall quality score (highest first) — markdown only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_quality_ranking_markdown: Vec<RankedFramework>,
    /// PDF-only: frameworks ranked by overall quality score (highest first) — plaintext only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_quality_ranking_plaintext: Vec<RankedFramework>,
    /// PDF-only: frameworks ranked by text F1 / TF1 (highest first) — markdown only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_tf1_ranking_markdown: Vec<RankedFramework>,
    /// PDF-only: frameworks ranked by text F1 / TF1 (highest first) — plaintext only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_tf1_ranking_plaintext: Vec<RankedFramework>,
    /// PDF-only: frameworks ranked by structural F1 / SF1 (highest first) — markdown only
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_sf1_ranking_markdown: Vec<RankedFramework>,
    /// Frameworks ranked by median pages/sec (highest first). Only frameworks with at least one
    /// PDF pages/sec observation are included (see `PerformancePercentiles.pages_per_sec`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages_per_sec_ranking: Vec<RankedFramework>,
    /// Frameworks ranked by median CPU-seconds consumed (lowest first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cpu_seconds_ranking: Vec<RankedFramework>,
    /// Performance deltas relative to the fastest framework (throughput-based)
    pub deltas_vs_baseline: HashMap<String, DeltaMetrics>,
    /// Non-dominated frontier over (pages/sec ↑, SF1 ↑, peak-RSS ↓), markdown frameworks only.
    /// See [`ParetoPoint`] for the dominance rule and eligibility criteria.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pareto_frontier: Vec<ParetoPoint>,
}

/// One non-dominated point in the (pages/sec, SF1, peak-RSS) multi-objective comparison.
///
/// A candidate is on the frontier when no other candidate **dominates** it: dominance requires
/// being at least as good on every objective and strictly better on at least one.
/// `pages_per_sec` and `sf1` are maximized; `peak_memory_mb` is minimized.
///
/// Restricted to markdown frameworks that have both an SF1 term and at least one pages/sec
/// observation: plaintext-only frameworks never carry SF1 (see module-level docs), and a
/// framework with no PDF page-count data has no pages/sec axis to compare on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParetoPoint {
    /// Framework:mode key, matching `RankedFramework.framework_mode`.
    pub framework_mode: String,
    /// Median pages/sec (higher is better).
    pub pages_per_sec: f64,
    /// Median structural F1 / SF1 (higher is better).
    pub sf1: f64,
    /// Median peak RSS in MB (lower is better).
    pub peak_memory_mb: f64,
}

/// A framework entry in a ranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedFramework {
    /// Framework:mode key (e.g., "xberg-markdown-baseline:single" or "docling:markdown:single")
    pub framework_mode: String,
    /// Rank (1-based)
    pub rank: usize,
    /// The metric value used for ranking
    pub value: f64,
    /// Ratio relative to the best in this ranking (1.0 = best)
    pub relative: f64,
}

/// Performance deltas relative to baseline (highest throughput framework)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaMetrics {
    /// Throughput delta in MB/s (negative = slower than baseline)
    pub throughput_delta_mbs: f64,
    /// Throughput delta as percentage relative to baseline
    pub throughput_delta_percent: f64,
    /// Memory delta in MB (positive = more memory than baseline)
    pub memory_delta_mb: f64,
    /// Memory delta as percentage relative to baseline
    pub memory_delta_percent: f64,
}

/// Metadata about the consolidation process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationMetadata {
    /// Number of benchmark results included
    pub total_results: usize,
    /// Number of unique frameworks
    pub framework_count: usize,
    /// Number of unique file types
    pub file_type_count: usize,
    /// File types the "overall" markdown quality ranking is actually computed over: the
    /// intersection of file types every markdown candidate framework attempted. When this
    /// degenerates to a single type (e.g. `["pdf"]`, because a PDF-only framework like
    /// liteparse/mineru is in the pool), `quality_ranking_markdown` is NOT a true all-format
    /// "overall" ranking — it reflects only these types. Consumers must read it accordingly.
    #[serde(default)]
    pub shared_corpus_markdown: Vec<String>,
    /// File types the "overall" plaintext quality ranking is computed over. Same semantics as
    /// [`Self::shared_corpus_markdown`].
    #[serde(default)]
    pub shared_corpus_plaintext: Vec<String>,
    /// Timestamp of consolidation
    pub timestamp: String,
}

/// Aggregated results for a specific framework, output format, and mode combination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkModeAggregation {
    /// Framework name (base name without mode suffix)
    pub framework: String,
    /// Output format (markdown or plaintext)
    pub output_format: OutputFormat,
    /// Mode: "single", "batch", "sync", "async"
    pub mode: String,
    /// Cold start duration statistics (if available)
    pub cold_start: Option<DurationPercentiles>,
    /// Process metrics deduplicated across all file-type and OCR buckets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overall_performance: Option<PerformancePercentiles>,
    /// Results grouped by file type
    pub by_file_type: HashMap<String, FileTypeAggregation>,
}

/// Aggregated results for a specific file type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeAggregation {
    /// File type (extension)
    pub file_type: String,
    /// Results without OCR
    pub no_ocr: Option<PerformancePercentiles>,
    /// Results with OCR
    pub with_ocr: Option<PerformancePercentiles>,
}

/// Performance percentiles for a group of results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformancePercentiles {
    /// Number of successful document samples used for quality calculations.
    pub successful_sample_count: usize,
    /// Number of process-level samples used for duration, throughput, and RSS.
    ///
    /// Native batches contribute one sample regardless of document cardinality.
    #[serde(default)]
    pub performance_sample_count: usize,
    /// Total number of samples in this group (including failed)
    pub total_sample_count: usize,
    /// Number of framework-side extraction errors (not our fault)
    pub framework_errors: usize,
    /// Number of harness-side errors (potentially our fault)
    pub harness_errors: usize,
    /// Number of configuration/setup errors (missing dependencies, env issues)
    pub config_setup_errors: usize,
    /// Number of extractions that timed out
    pub timeouts: usize,
    /// Number of extractions that returned empty content
    pub empty_content: usize,
    /// Unique error messages with occurrence counts
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub error_details: HashMap<String, usize>,
    /// Throughput percentiles (p50, p95, p99) in MB/s
    pub throughput: Percentiles,
    /// Memory percentiles (p50, p95, p99) in MB
    pub memory: Percentiles,
    /// Duration percentiles (p50, p95, p99) in ms
    pub duration: Percentiles,
    /// Success rate as percentage (0-100)
    pub success_rate_percent: f64,
    /// Extraction duration percentiles (p50, p95, p99) in ms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_duration: Option<Percentiles>,
    /// Quality score percentiles (p50, p95, p99) — 0.0 to 1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityPercentiles>,
    /// Pages-per-second percentiles, derived from `PdfMetadata.page_count` divided by wall-clock
    /// duration. `None` when no result in this group carries a known PDF page count (e.g.
    /// non-PDF file types, or a page count the harness could not detect). For a native batch,
    /// the page counts of every document sharing one `batch_sample_id` are summed and divided by
    /// the shared batch makespan, mirroring how `throughput` is computed for batches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages_per_sec: Option<Percentiles>,
    /// Total process-tree CPU-time percentiles, in core-seconds
    /// (see `PerformanceMetrics::cpu_seconds` for the integration methodology and its
    /// sample-interval-bounded precision).
    #[serde(default)]
    pub cpu_seconds: Percentiles,
    /// Approximate number of documents processed per one measured process invocation in this
    /// group: `Some(1)` for single-file mode; for batch mode, the modal document count per
    /// deduped performance sample (`total_sample_count / performance_sample_count`, rounded).
    /// `None` when the group has no successful performance samples to derive a ratio from.
    /// Surfaced so peak-RSS (and other performance metrics) can be read "keyed by batch size"
    /// without adding a new axis to the `by_framework_mode` aggregate key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
    /// System-load contention qualifier aggregated from `BenchmarkResult.system_load` samples in
    /// this group. `None` when no result in the group carries a load snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_load: Option<SystemLoadPercentiles>,
}

/// Aggregated system-load contention qualifier for a group of results.
///
/// Lets a consumer judge whether a bucket's timing data is comparable to an idle-machine
/// baseline: see `crate::system_load::SystemLoad` for why load figures are read *relatively*
/// (was this bucket measured under similar or worse contention than another) rather than as an
/// absolute number.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLoadPercentiles {
    /// 50th percentile of `SystemLoad::load_per_core()` across the group's samples.
    pub load_per_core_p50: f64,
    /// 95th percentile of `SystemLoad::load_per_core()` across the group's samples.
    pub load_per_core_p95: f64,
    /// Number of samples for which `SystemLoad::is_contended()` was true.
    pub contended_sample_count: usize,
    /// Total number of results in the group carrying a system-load snapshot.
    pub total_sample_count: usize,
}

/// Quality percentile values (p50, p95, p99) for all F1 metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityPercentiles {
    /// Text F1 50th percentile (TF1 median)
    pub f1_text_p50: f64,
    /// Text F1 95th percentile
    pub f1_text_p95: f64,
    /// Text F1 99th percentile
    pub f1_text_p99: f64,
    /// Numeric F1 50th percentile
    pub f1_numeric_p50: f64,
    /// Numeric F1 95th percentile
    pub f1_numeric_p95: f64,
    /// Numeric F1 99th percentile
    pub f1_numeric_p99: f64,
    /// Layout/structural F1 50th percentile (SF1 median) — None for plaintext-only frameworks
    pub f1_layout_p50: Option<f64>,
    /// Layout/structural F1 95th percentile — None for plaintext-only frameworks
    pub f1_layout_p95: Option<f64>,
    /// Layout/structural F1 99th percentile — None for plaintext-only frameworks
    pub f1_layout_p99: Option<f64>,
    /// Overall quality score 50th percentile
    pub quality_score_p50: f64,
    /// Overall quality score 95th percentile
    pub quality_score_p95: f64,
    /// Overall quality score 99th percentile
    pub quality_score_p99: f64,
}

/// Percentile values for a metric
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Percentiles {
    /// 50th percentile (median)
    pub p50: f64,
    /// 95th percentile
    pub p95: f64,
    /// 99th percentile
    pub p99: f64,
}

/// Duration percentiles in milliseconds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurationPercentiles {
    /// Number of samples with cold start data
    pub sample_count: usize,
    /// 50th percentile (median) in ms
    pub p50_ms: f64,
    /// 95th percentile in ms
    pub p95_ms: f64,
    /// 99th percentile in ms
    pub p99_ms: f64,
}

/// Main aggregation function for new format
///
/// Groups results by:
/// 1. Framework and mode (extracted from framework name)
/// 2. File type (extension)
/// 3. OCR usage (yes/no)
///
/// Calculates p50/p95/p99 percentiles for each group.
pub fn aggregate_new_format(results: &[BenchmarkResult]) -> NewConsolidatedResults {
    if results.is_empty() {
        return NewConsolidatedResults {
            schema_version: SCHEMA_VERSION.to_string(),
            by_framework_mode: HashMap::new(),
            disk_sizes: HashMap::new(),
            comparison: ComparisonData {
                throughput_ranking: Vec::new(),
                memory_ranking: Vec::new(),
                quality_ranking_markdown: Vec::new(),
                quality_ranking_plaintext: Vec::new(),
                pdf_quality_ranking_markdown: Vec::new(),
                pdf_quality_ranking_plaintext: Vec::new(),
                pdf_tf1_ranking_markdown: Vec::new(),
                pdf_tf1_ranking_plaintext: Vec::new(),
                pdf_sf1_ranking_markdown: Vec::new(),
                pages_per_sec_ranking: Vec::new(),
                cpu_seconds_ranking: Vec::new(),
                deltas_vs_baseline: HashMap::new(),
                pareto_frontier: Vec::new(),
            },
            per_fixture_results: Vec::new(),
            metadata: ConsolidationMetadata {
                total_results: 0,
                framework_count: 0,
                file_type_count: 0,
                shared_corpus_markdown: Vec::new(),
                shared_corpus_plaintext: Vec::new(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
        };
    }

    let mut by_framework_mode_format: HashMap<String, HashMap<String, Vec<&BenchmarkResult>>> = HashMap::new();
    let mut disk_sizes: HashMap<String, DiskSizeInfo> = HashMap::new();
    let mut file_types = std::collections::HashSet::new();

    for result in results {
        let (framework, mode) = extract_framework_and_mode(&result.framework);
        let key = make_aggregate_key(framework, result.output_format, mode);

        by_framework_mode_format
            .entry(key)
            .or_default()
            .entry(result.file_extension.clone())
            .or_default()
            .push(result);

        file_types.insert(result.file_extension.clone());

        if let Some(disk_size) = &result.framework_capabilities.installation_size {
            disk_sizes.insert(framework.to_string(), disk_size.clone());
        }
    }

    let mut aggregated_by_framework_mode = HashMap::new();

    for (framework_mode_format_key, file_type_results) in by_framework_mode_format {
        let output_format = file_type_results
            .values()
            .flatten()
            .next()
            .map(|r| r.output_format)
            .unwrap_or(OutputFormat::Markdown);

        let (framework, mode) = parse_aggregate_key(&framework_mode_format_key);

        let all_results: Vec<&BenchmarkResult> = file_type_results.values().flat_map(|v| v.iter().copied()).collect();
        let cold_start = aggregate_cold_starts(&all_results);
        let overall_performance = Some(calculate_percentiles(&all_results));

        let mut by_file_type = HashMap::new();
        for (file_type, results_for_type) in file_type_results {
            let aggregation = aggregate_by_ocr_status(&results_for_type);
            by_file_type.insert(
                file_type.clone(),
                FileTypeAggregation {
                    file_type: file_type.clone(),
                    no_ocr: aggregation.0,
                    with_ocr: aggregation.1,
                },
            );
        }

        aggregated_by_framework_mode.insert(
            framework_mode_format_key.clone(),
            FrameworkModeAggregation {
                framework: framework.to_string(),
                output_format,
                mode: mode.to_string(),
                cold_start,
                overall_performance,
                by_file_type,
            },
        );
    }

    let per_fixture_results = build_per_fixture_results(results);

    // Count *logical* frameworks: all xberg pipelines (xberg-markdown-baseline,
    // xberg-plaintext-layout, …) are variants of the single "xberg" framework, so collapse
    // them to one before counting. Otherwise framework_count over-reports by the number of
    // xberg name-variants present (e.g. 11 instead of 8). ~keep
    let framework_count = results
        .iter()
        .map(|r| {
            let name = extract_framework_and_mode(&r.framework).0;
            if name.starts_with("xberg") { "xberg" } else { name }
        })
        .collect::<std::collections::HashSet<_>>()
        .len();

    let metadata = ConsolidationMetadata {
        total_results: results.len(),
        framework_count,
        file_type_count: file_types.len(),
        shared_corpus_markdown: resolve_shared_corpus_file_types(&aggregated_by_framework_mode, OutputFormat::Markdown),
        shared_corpus_plaintext: resolve_shared_corpus_file_types(
            &aggregated_by_framework_mode,
            OutputFormat::Plaintext,
        ),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let comparison = build_comparison(&aggregated_by_framework_mode);

    NewConsolidatedResults {
        schema_version: SCHEMA_VERSION.to_string(),
        by_framework_mode: aggregated_by_framework_mode,
        disk_sizes,
        comparison,
        per_fixture_results,
        metadata,
    }
}

/// Build per-fixture result rows from raw benchmark results
///
/// Extracts one row per (framework, output_format, execution_mode, fixture_id, ocr) group.
/// Fixture ID is derived from the file path (filename without extension).
fn build_per_fixture_results(results: &[BenchmarkResult]) -> Vec<PerFixtureRow> {
    let mut fixture_rows = Vec::new();

    for result in results {
        let (framework, mode) = extract_framework_and_mode(&result.framework);
        let fixture_id = result
            .file_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_string();

        let ocr = match result.ocr_status {
            crate::types::OcrStatus::Used => Some(true),
            crate::types::OcrStatus::NotUsed => Some(false),
            crate::types::OcrStatus::Unknown => None,
        };
        let error_kind = if !result.success {
            Some(format!("{:?}", result.error_kind))
        } else {
            None
        };

        let (f1_text, f1_layout, f1_numeric, quality_score, correct) = if let Some(q) = &result.quality {
            (
                Some(q.f1_score_text),
                q.f1_score_layout,
                Some(q.f1_score_numeric),
                Some(q.quality_score),
                Some(q.correct),
            )
        } else {
            (None, None, None, None, None)
        };

        fixture_rows.push(PerFixtureRow {
            framework: framework.to_string(),
            output_format: result.output_format,
            execution_mode: mode.to_string(),
            ocr,
            fixture_id,
            file_type: result.file_extension.clone(),
            duration_ms: result.duration.as_secs_f64() * 1000.0,
            peak_memory_mb: result.metrics.peak_memory_bytes as f64 / 1_000_000.0,
            f1_text,
            f1_layout,
            f1_numeric,
            quality_score,
            correct,
            success: result.success,
            error_kind,
        });
    }

    fixture_rows
}

/// Aggregate results by OCR status
///
/// Returns (no_ocr, with_ocr) tuple of PerformancePercentiles
fn aggregate_by_ocr_status(
    results: &[&BenchmarkResult],
) -> (Option<PerformancePercentiles>, Option<PerformancePercentiles>) {
    use crate::types::OcrStatus;

    // Unknown is deliberately excluded from both cohorts. In particular, an
    // unreported PDF OCR status must never be presented as a no-OCR result. ~keep
    let no_ocr: Vec<&BenchmarkResult> = results
        .iter()
        .filter(|result| result.ocr_status == OcrStatus::NotUsed)
        .copied()
        .collect();

    let with_ocr: Vec<&BenchmarkResult> = results
        .iter()
        .filter(|result| result.ocr_status == OcrStatus::Used)
        .copied()
        .collect();

    let no_ocr_stats = if !no_ocr.is_empty() {
        Some(calculate_percentiles(&no_ocr))
    } else {
        None
    };

    let with_ocr_stats = if !with_ocr.is_empty() {
        Some(calculate_percentiles(&with_ocr))
    } else {
        None
    };

    (no_ocr_stats, with_ocr_stats)
}

/// Calculate percentiles for a group of results
///
/// Only uses successful results for metric calculations.
/// Success rate is calculated from all results.
fn calculate_percentiles(results: &[&BenchmarkResult]) -> PerformancePercentiles {
    let successful: Vec<&BenchmarkResult> = results.iter().filter(|r| r.success).copied().collect();
    let performance_samples = successful_performance_samples(results.iter().copied());

    let mut durations: Vec<f64> = performance_samples
        .iter()
        .map(|r| r.duration.as_secs_f64() * 1000.0)
        .filter(|&v| !v.is_nan() && v.is_finite())
        .collect();

    let mut throughputs: Vec<f64> = performance_samples
        .iter()
        .map(|r| r.metrics.throughput_bytes_per_sec / 1_000_000.0)
        .filter(|&v| v > 0.0 && v.is_finite())
        .collect();

    let mut memories: Vec<f64> = performance_samples
        .iter()
        .map(|r| r.metrics.peak_memory_bytes as f64 / 1_000_000.0)
        .filter(|&v| !v.is_nan() && v.is_finite())
        .collect();

    let mut extraction_durations: Vec<f64> = successful
        .iter()
        .filter_map(|r| r.extraction_duration.map(|d| d.as_secs_f64() * 1000.0))
        .filter(|&v| !v.is_nan() && v.is_finite())
        .collect();

    let mut cpu_seconds_values: Vec<f64> = performance_samples
        .iter()
        .map(|r| r.metrics.cpu_seconds)
        .filter(|&v| !v.is_nan() && v.is_finite() && v >= 0.0)
        .collect();

    let mut pages_per_sec_values = collect_pages_per_second(&successful);

    durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    throughputs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    memories.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    extraction_durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cpu_seconds_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    pages_per_sec_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let duration = Percentiles {
        p50: sanitize_f64(percentile_r7(&durations, 0.50)),
        p95: sanitize_f64(percentile_r7(&durations, 0.95)),
        p99: sanitize_f64(percentile_r7(&durations, 0.99)),
    };

    let throughput = Percentiles {
        p50: sanitize_f64(percentile_r7(&throughputs, 0.50)),
        p95: sanitize_f64(percentile_r7(&throughputs, 0.95)),
        p99: sanitize_f64(percentile_r7(&throughputs, 0.99)),
    };

    let memory = Percentiles {
        p50: sanitize_f64(percentile_r7(&memories, 0.50)),
        p95: sanitize_f64(percentile_r7(&memories, 0.95)),
        p99: sanitize_f64(percentile_r7(&memories, 0.99)),
    };

    let extraction_duration = if !extraction_durations.is_empty() {
        Some(Percentiles {
            p50: sanitize_f64(percentile_r7(&extraction_durations, 0.50)),
            p95: sanitize_f64(percentile_r7(&extraction_durations, 0.95)),
            p99: sanitize_f64(percentile_r7(&extraction_durations, 0.99)),
        })
    } else {
        None
    };

    let cpu_seconds = Percentiles {
        p50: sanitize_f64(percentile_r7(&cpu_seconds_values, 0.50)),
        p95: sanitize_f64(percentile_r7(&cpu_seconds_values, 0.95)),
        p99: sanitize_f64(percentile_r7(&cpu_seconds_values, 0.99)),
    };

    let pages_per_sec = if !pages_per_sec_values.is_empty() {
        Some(Percentiles {
            p50: sanitize_f64(percentile_r7(&pages_per_sec_values, 0.50)),
            p95: sanitize_f64(percentile_r7(&pages_per_sec_values, 0.95)),
            p99: sanitize_f64(percentile_r7(&pages_per_sec_values, 0.99)),
        })
    } else {
        None
    };

    // Approximate documents-per-process-invocation: 1:1 for single-file mode (each successful
    // row is its own performance sample), or the modal batch size for native batches (deduped
    // performance samples each represent one whole-batch process). See `PerformancePercentiles`
    // doc comment. ~keep
    let batch_size = if performance_samples.is_empty() {
        None
    } else {
        Some(
            (results.len() as f64 / performance_samples.len() as f64)
                .round()
                .max(1.0) as usize,
        )
    };

    let system_load = aggregate_system_load(results);

    let success_rate_percent = if !results.is_empty() {
        (successful.len() as f64 / results.len() as f64) * 100.0
    } else {
        0.0
    };

    let framework_errors = results
        .iter()
        .filter(|r| r.error_kind == ErrorKind::FrameworkError)
        .count();
    let harness_errors = results
        .iter()
        .filter(|r| r.error_kind == ErrorKind::HarnessError)
        .count();
    let config_setup_errors = results
        .iter()
        .filter(|r| r.error_kind == ErrorKind::ConfigSetupError)
        .count();
    let timeouts = results.iter().filter(|r| r.error_kind == ErrorKind::Timeout).count();
    let empty_content = results
        .iter()
        .filter(|r| r.error_kind == ErrorKind::EmptyContent)
        .count();

    let mut error_details: HashMap<String, usize> = HashMap::new();
    for result in results.iter().filter(|r| !r.success) {
        if let Some(msg) = &result.error_message {
            *error_details.entry(msg.clone()).or_insert(0) += 1;
        }
    }

    let quality = {
        let mut f1_texts: Vec<f64> = successful
            .iter()
            .filter_map(|r| r.quality.as_ref().map(|q| q.f1_score_text))
            .filter(|v| !v.is_nan() && v.is_finite())
            .collect();
        let mut f1_numerics: Vec<f64> = successful
            .iter()
            .filter_map(|r| r.quality.as_ref().map(|q| q.f1_score_numeric))
            .filter(|v| !v.is_nan() && v.is_finite())
            .collect();
        let mut f1_layouts: Vec<f64> = successful
            .iter()
            .filter_map(|r| r.quality.as_ref().and_then(|q| q.f1_score_layout))
            .filter(|v| !v.is_nan() && v.is_finite())
            .collect();
        let mut quality_scores: Vec<f64> = successful
            .iter()
            .filter_map(|r| r.quality.as_ref().map(|q| q.quality_score))
            .filter(|v| !v.is_nan() && v.is_finite())
            .collect();

        if !quality_scores.is_empty() {
            f1_texts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            f1_numerics.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            f1_layouts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            quality_scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let f1_layout_p50 = if !f1_layouts.is_empty() {
                Some(sanitize_f64(percentile_r7(&f1_layouts, 0.50)))
            } else {
                None
            };
            let f1_layout_p95 = if !f1_layouts.is_empty() {
                Some(sanitize_f64(percentile_r7(&f1_layouts, 0.95)))
            } else {
                None
            };
            let f1_layout_p99 = if !f1_layouts.is_empty() {
                Some(sanitize_f64(percentile_r7(&f1_layouts, 0.99)))
            } else {
                None
            };

            Some(QualityPercentiles {
                f1_text_p50: sanitize_f64(percentile_r7(&f1_texts, 0.50)),
                f1_text_p95: sanitize_f64(percentile_r7(&f1_texts, 0.95)),
                f1_text_p99: sanitize_f64(percentile_r7(&f1_texts, 0.99)),
                f1_numeric_p50: sanitize_f64(percentile_r7(&f1_numerics, 0.50)),
                f1_numeric_p95: sanitize_f64(percentile_r7(&f1_numerics, 0.95)),
                f1_numeric_p99: sanitize_f64(percentile_r7(&f1_numerics, 0.99)),
                f1_layout_p50,
                f1_layout_p95,
                f1_layout_p99,
                quality_score_p50: sanitize_f64(percentile_r7(&quality_scores, 0.50)),
                quality_score_p95: sanitize_f64(percentile_r7(&quality_scores, 0.95)),
                quality_score_p99: sanitize_f64(percentile_r7(&quality_scores, 0.99)),
            })
        } else {
            None
        }
    };

    PerformancePercentiles {
        successful_sample_count: successful.len(),
        performance_sample_count: performance_samples.len(),
        total_sample_count: results.len(),
        framework_errors,
        harness_errors,
        config_setup_errors,
        timeouts,
        empty_content,
        error_details,
        throughput,
        memory,
        duration,
        success_rate_percent,
        extraction_duration,
        quality,
        pages_per_sec,
        cpu_seconds,
        batch_size,
        system_load,
    }
}

/// Compute one pages/sec observation per performance sample (see [`successful_performance_samples`]).
///
/// For a single-file result, this is simply `page_count / duration`. For a native batch, every
/// document sharing one `batch_sample_id` has its own `PdfMetadata.page_count`, so the *total*
/// pages processed by that one batch invocation is summed across all its member rows before
/// dividing by the (shared) batch makespan — mirroring how batch-wide `throughput_bytes_per_sec`
/// is computed from summed bytes, not a single member row's byte count.
///
/// Rows without a detected PDF page count (non-PDF files, or a PDF the harness could not size)
/// are excluded rather than treated as zero.
fn collect_pages_per_second(successful: &[&BenchmarkResult]) -> Vec<f64> {
    let mut batch_pages: HashMap<&str, u64> = HashMap::new();
    for result in successful {
        if let (Some(batch_id), Some(page_count)) = (
            result.framework_capabilities.batch_sample_id.as_deref(),
            result.pdf_metadata.as_ref().and_then(|metadata| metadata.page_count),
        ) {
            *batch_pages.entry(batch_id).or_insert(0) += page_count as u64;
        }
    }

    successful_performance_samples(successful.iter().copied())
        .into_iter()
        .filter_map(|sample| {
            let duration_secs = sample.duration.as_secs_f64();
            if duration_secs <= 0.0 {
                return None;
            }
            let pages = match sample.framework_capabilities.batch_sample_id.as_deref() {
                Some(batch_id) => *batch_pages.get(batch_id)?,
                None => sample.pdf_metadata.as_ref().and_then(|metadata| metadata.page_count)? as u64,
            };
            if pages == 0 {
                return None;
            }
            Some(pages as f64 / duration_secs)
        })
        .collect()
}

/// Aggregate the `SystemLoad` snapshots carried by a group of results into a contention
/// qualifier. Returns `None` when no result in the group recorded a snapshot.
fn aggregate_system_load(results: &[&BenchmarkResult]) -> Option<SystemLoadPercentiles> {
    let mut load_per_core: Vec<f64> = results
        .iter()
        .filter_map(|r| r.system_load.as_ref())
        .map(|load| load.load_per_core())
        .filter(|v| !v.is_nan() && v.is_finite())
        .collect();

    if load_per_core.is_empty() {
        return None;
    }

    let contended_sample_count = results
        .iter()
        .filter_map(|r| r.system_load.as_ref())
        .filter(|load| load.is_contended())
        .count();

    load_per_core.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    Some(SystemLoadPercentiles {
        load_per_core_p50: sanitize_f64(percentile_r7(&load_per_core, 0.50)),
        load_per_core_p95: sanitize_f64(percentile_r7(&load_per_core, 0.95)),
        contended_sample_count,
        total_sample_count: load_per_core.len(),
    })
}

/// Aggregate cold start durations
///
/// Returns percentiles of cold start durations if any results have cold start data.
fn aggregate_cold_starts(results: &[&BenchmarkResult]) -> Option<DurationPercentiles> {
    let cold_starts: Vec<f64> = results
        .iter()
        .filter_map(|r| r.cold_start_duration.map(|d| d.as_secs_f64() * 1000.0))
        .filter(|&v| !v.is_nan() && v.is_finite())
        .collect();

    if cold_starts.is_empty() {
        return None;
    }

    let mut sorted = cold_starts.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    Some(DurationPercentiles {
        sample_count: cold_starts.len(),
        p50_ms: sanitize_f64(percentile_r7(&sorted, 0.50)),
        p95_ms: sanitize_f64(percentile_r7(&sorted, 0.95)),
        p99_ms: sanitize_f64(percentile_r7(&sorted, 0.99)),
    })
}

/// Extract framework name and mode from a raw framework string.
///
/// Modes: `-batch` suffix → `"batch"`, anything else → `"single"`.
/// Legacy `-sync`/`-async` suffixes (no longer emitted by current adapters, but present in
/// historical result files) are stripped from the base name to preserve backward compatibility.
///
/// Returns `(framework_name, mode)` where `mode` is `"batch"` or `"single"`.
fn extract_framework_and_mode(framework_name: &str) -> (&str, &str) {
    if let Some(base) = framework_name.strip_suffix("-batch") {
        let normalized = base
            .strip_suffix("-sync")
            .or_else(|| base.strip_suffix("-async"))
            .unwrap_or(base);
        (normalized, "batch")
    } else {
        let normalized = framework_name
            .strip_suffix("-sync")
            .or_else(|| framework_name.strip_suffix("-async"))
            .unwrap_or(framework_name);
        (normalized, "single")
    }
}

/// Build the `by_framework_mode` map key for a result.
///
/// - `xberg-*` frameworks already encode the output format in their name, so the key is
///   `"{framework}:{mode}"` — no redundant format component.
/// - All other (competitor) frameworks use `"{framework}:{output_format}:{mode}"`.
pub(crate) fn make_aggregate_key(framework: &str, output_format: OutputFormat, mode: &str) -> String {
    if framework.starts_with("xberg-") {
        format!("{framework}:{mode}")
    } else {
        format!("{framework}:{output_format}:{mode}")
    }
}

/// Parse an aggregate key back into `(framework, mode)`.
///
/// Handles both key shapes produced by [`make_aggregate_key`]:
/// - `"framework:mode"` (xberg family, 2 parts)
/// - `"framework:output_format:mode"` (competitors, 3 parts)
fn parse_aggregate_key(key: &str) -> (&str, &str) {
    let mut parts = key.rsplitn(2, ':');
    let mode = parts.next().unwrap_or("single");
    let remainder = parts.next().unwrap_or(key);
    let framework = remainder.split(':').next().unwrap_or(remainder);
    (framework, mode)
}

/// Weighted mean of `(value, weight)` pairs, ignoring non-finite values. Returns `NaN` if no
/// finite-weighted contribution exists (e.g. every value was non-finite, or the slice was empty).
fn weighted_avg(items: &[(f64, usize)]) -> f64 {
    let finite: Vec<(f64, usize)> = items.iter().copied().filter(|(v, _)| v.is_finite()).collect();
    let total_weight: usize = finite.iter().map(|(_, w)| w).sum();
    if total_weight == 0 {
        f64::NAN
    } else {
        finite.iter().map(|(v, w)| v * (*w as f64)).sum::<f64>() / total_weight as f64
    }
}

/// Build the overall (all-file-types) quality ranking for one output format, restricted to a
/// **shared corpus** and counting fully-failed buckets against the framework.
///
/// # Semantics (Bug A: mismatched per-framework corpora)
///
/// Frameworks in real benchmark runs attempt wildly different sets of file types (e.g.
/// `liteparse` is PDF-only, `docling` never attempts `json`/`txt`, `xberg` runs the full
/// corpus). Naively weighting each framework's own quality mean by whatever file types *it*
/// happened to attempt makes the "overall" ranking compare non-comparable bases — a framework
/// that only ever attempted its best file type would look artificially strong.
///
/// The fix: restrict the overall ranking to the **intersection of file types every candidate
/// framework (of this output format) attempted** — "attempted" meaning `total_sample_count > 0`
/// in at least one of `no_ocr`/`with_ocr` for that file type, regardless of success. Only that
/// shared set feeds the weighted mean, so every ranked framework is scored on the same corpus.
///
/// With a single candidate framework for a format, the "intersection" is trivially that
/// framework's own attempted file types — there is nothing to restrict against, so it is ranked
/// on everything it ran (rank 1 by construction). The shared-corpus restriction only bites once
/// two or more frameworks of the same format disagree on which file types they attempted.
///
/// Judgment call: if the shared set is empty (no candidates, or candidates share no file type at
/// all), there is no meaningful "overall" comparison to make — this function returns an empty
/// ranking rather than fabricating one from a partial/non-shared basis. Callers should treat an
/// empty result as "no shared-corpus overall ranking available for this format" and rely on the
/// per-file-type (e.g. `pdf_*`) rankings instead.
///
/// # Semantics (Bug B: 0-success buckets silently dropped)
///
/// Within the shared file-type set, a bucket a framework *attempted but completely failed*
/// (`successful_sample_count == 0`, `total_sample_count > 0`) must drag its mean down — it is
/// not neutral, it is a failure. Such buckets contribute a quality value of `0.0`, weighted by
/// the bucket's `total_sample_count` (samples attempted, not just those that happened to
/// succeed). This is distinct from a file type the framework never attempted at all, which is
/// excluded entirely by the shared-corpus restriction above (that's not a failure, it's missing
/// data, and including it would penalize frameworks for corpora they were never run against).
/// Resolve the shared corpus for a format: the file types every candidate framework of that
/// format actually attempted (any `total_sample_count > 0` in either OCR bucket), intersected
/// across all candidates. This is the exact basis on which the "overall" quality ranking for
/// the format is computed; a single-format framework in the pool collapses it to that one type.
/// Returned sorted for stable metadata output.
fn resolve_shared_corpus_file_types(
    by_framework_mode: &HashMap<String, FrameworkModeAggregation>,
    format: OutputFormat,
) -> Vec<String> {
    let mut shared_file_types: Option<std::collections::HashSet<&str>> = None;
    for agg in by_framework_mode.values().filter(|agg| agg.output_format == format) {
        let attempted: std::collections::HashSet<&str> = agg
            .by_file_type
            .iter()
            .filter(|(_, ft)| {
                [&ft.no_ocr, &ft.with_ocr]
                    .into_iter()
                    .flatten()
                    .any(|perf| perf.total_sample_count > 0)
            })
            .map(|(file_type, _)| file_type.as_str())
            .collect();
        shared_file_types = Some(match shared_file_types {
            Some(existing) => existing.intersection(&attempted).copied().collect(),
            None => attempted,
        });
    }
    let mut out: Vec<String> = shared_file_types
        .unwrap_or_default()
        .into_iter()
        .map(String::from)
        .collect();
    out.sort();
    out
}

fn build_shared_corpus_quality_ranking(
    by_framework_mode: &HashMap<String, FrameworkModeAggregation>,
    format: OutputFormat,
) -> Vec<RankedFramework> {
    let candidates: Vec<(&String, &FrameworkModeAggregation)> = by_framework_mode
        .iter()
        .filter(|(_, agg)| agg.output_format == format)
        .collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    let shared_file_types = resolve_shared_corpus_file_types(by_framework_mode, format);

    if shared_file_types.is_empty() {
        return Vec::new();
    }

    let mut qual: Vec<(String, f64)> = Vec::new();
    for (key, agg) in candidates {
        let mut contributions: Vec<(f64, usize)> = Vec::new();
        for file_type in &shared_file_types {
            let Some(ft) = agg.by_file_type.get(file_type.as_str()) else {
                continue;
            };
            for perf in [&ft.no_ocr, &ft.with_ocr].into_iter().flatten() {
                if perf.total_sample_count == 0 {
                    continue;
                }
                let weight = perf.total_sample_count;
                let value = perf.quality.as_ref().map(|q| q.quality_score_p50).unwrap_or(0.0);
                contributions.push((value, weight));
            }
        }
        let mean = weighted_avg(&contributions);
        if mean.is_finite() {
            qual.push((key.clone(), mean));
        }
    }

    qual.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let baseline_qual = qual.first().map(|r| r.1).unwrap_or(1.0);
    qual.iter()
        .enumerate()
        .map(|(i, (k, v))| RankedFramework {
            framework_mode: k.clone(),
            rank: i + 1,
            value: *v,
            relative: if baseline_qual > 0.0 { *v / baseline_qual } else { 1.0 },
        })
        .collect()
}

/// Build cross-framework comparison rankings from aggregated data
///
/// Uses the framework-mode-wide process aggregation so a native batch contributes
/// once even when its document rows span several file-type or OCR buckets.
fn build_comparison(by_framework_mode: &HashMap<String, FrameworkModeAggregation>) -> ComparisonData {
    let mut metrics: Vec<(String, f64, f64, OutputFormat)> = Vec::new();
    let mut cpu_seconds_metrics: Vec<(String, f64)> = Vec::new();
    let mut pages_per_sec_metrics: Vec<(String, f64)> = Vec::new();

    for (key, agg) in by_framework_mode {
        let Some(performance) = agg
            .overall_performance
            .as_ref()
            .filter(|performance| performance.performance_sample_count > 0)
        else {
            continue;
        };

        metrics.push((
            key.clone(),
            performance.throughput.p50,
            performance.memory.p50,
            agg.output_format,
        ));
        cpu_seconds_metrics.push((key.clone(), performance.cpu_seconds.p50));
        if let Some(pages_per_sec) = &performance.pages_per_sec {
            pages_per_sec_metrics.push((key.clone(), pages_per_sec.p50));
        }
    }

    let mut thr = metrics.clone();
    thr.retain(|m| m.1.is_finite());
    thr.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let baseline_thr = thr.first().map(|r| r.1).unwrap_or(1.0);
    let throughput_ranking: Vec<RankedFramework> = thr
        .iter()
        .enumerate()
        .map(|(i, (k, v, ..))| RankedFramework {
            framework_mode: k.clone(),
            rank: i + 1,
            value: *v,
            relative: if baseline_thr > 0.0 { *v / baseline_thr } else { 1.0 },
        })
        .collect();

    let mut mem = metrics.clone();
    mem.retain(|m| m.2.is_finite());
    mem.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let baseline_mem = mem.first().map(|r| r.2).unwrap_or(1.0);
    let memory_ranking: Vec<RankedFramework> = mem
        .iter()
        .enumerate()
        .map(|(i, (k, _, v, _))| RankedFramework {
            framework_mode: k.clone(),
            rank: i + 1,
            value: *v,
            relative: if baseline_mem > 0.0 { *v / baseline_mem } else { 1.0 },
        })
        .collect();

    let quality_ranking_markdown = build_shared_corpus_quality_ranking(by_framework_mode, OutputFormat::Markdown);
    let quality_ranking_plaintext = build_shared_corpus_quality_ranking(by_framework_mode, OutputFormat::Plaintext);

    let mut deltas_vs_baseline = HashMap::new();
    if let Some(baseline) = metrics
        .iter()
        .filter(|(_, thr, _, _)| thr.is_finite())
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    {
        for (k, thr, mem_val, _) in &metrics {
            if k != &baseline.0 {
                deltas_vs_baseline.insert(
                    k.clone(),
                    DeltaMetrics {
                        throughput_delta_mbs: thr - baseline.1,
                        throughput_delta_percent: if baseline.1 > 0.0 {
                            ((thr - baseline.1) / baseline.1) * 100.0
                        } else {
                            0.0
                        },
                        memory_delta_mb: mem_val - baseline.2,
                        memory_delta_percent: if baseline.2 > 0.0 {
                            ((mem_val - baseline.2) / baseline.2) * 100.0
                        } else {
                            0.0
                        },
                    },
                );
            }
        }
    }

    // Bug B: a PDF bucket a framework *attempted but completely failed*
    // (`successful_sample_count == 0`, `total_sample_count > 0`) must count against it — quality
    // contribution 0.0, weighted by samples attempted (not just those that succeeded) — instead
    // of being silently dropped (which let e.g. a framework failing 100% of PDFs escape any
    // quality penalty). TF1/SF1 use the same 0.0-on-full-failure treatment for consistency. ~keep
    let mut pdf_metrics: Vec<(String, f64, f64, f64, OutputFormat)> = Vec::new();
    for (key, agg) in by_framework_mode {
        if let Some(pdf_ft) = agg.by_file_type.get("pdf") {
            let mut qualities: Vec<(f64, usize)> = Vec::new();
            let mut tf1s: Vec<(f64, usize)> = Vec::new();
            let mut sf1s: Vec<(f64, usize)> = Vec::new();
            for perf in [&pdf_ft.no_ocr, &pdf_ft.with_ocr].into_iter().flatten() {
                if perf.total_sample_count == 0 {
                    continue;
                }
                let w = perf.total_sample_count;
                let (quality_value, tf1_value, sf1_value) = match &perf.quality {
                    Some(q) => (q.quality_score_p50, q.f1_text_p50, q.f1_layout_p50),
                    None => (0.0, 0.0, None),
                };
                qualities.push((quality_value, w));
                tf1s.push((tf1_value, w));
                // SF1 has no defined "failure" value for plaintext-only frameworks (they never
                // carry a layout term at all), so a missing layout score only contributes 0.0
                // when the bucket was a genuine failure (no quality at all), not when the
                // framework is plaintext-only and layout is simply not applicable. ~keep
                match sf1_value {
                    Some(layout) => sf1s.push((layout, w)),
                    None if perf.quality.is_none() => sf1s.push((0.0, w)),
                    None => {}
                }
            }
            let q = weighted_avg(&qualities);
            let t = weighted_avg(&tf1s);
            let s = weighted_avg(&sf1s);
            if q.is_finite() {
                pdf_metrics.push((key.clone(), q, t, s, agg.output_format));
            }
        }
    }

    let build_ranking = |items: &mut Vec<(String, f64)>| -> Vec<RankedFramework> {
        items.retain(|(_, v)| v.is_finite());
        items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let best = items.first().map(|r| r.1).unwrap_or(1.0);
        items
            .iter()
            .enumerate()
            .map(|(i, (k, v))| RankedFramework {
                framework_mode: k.clone(),
                rank: i + 1,
                value: *v,
                relative: if best > 0.0 { *v / best } else { 1.0 },
            })
            .collect()
    };

    // As with the all-file-types quality ranking above, PDF quality must also be split by
    // output format — plaintext-only frameworks never carry an SF1 term and must never be
    // pooled against markdown frameworks' layout-inclusive quality score. ~keep
    let mut pdf_qual_markdown: Vec<(String, f64)> = pdf_metrics
        .iter()
        .filter(|(_, _, _, _, fmt)| *fmt == OutputFormat::Markdown)
        .map(|(k, q, _, _, _)| (k.clone(), *q))
        .collect();
    let mut pdf_qual_plaintext: Vec<(String, f64)> = pdf_metrics
        .iter()
        .filter(|(_, _, _, _, fmt)| *fmt == OutputFormat::Plaintext)
        .map(|(k, q, _, _, _)| (k.clone(), *q))
        .collect();
    let mut pdf_tf1_markdown: Vec<(String, f64)> = pdf_metrics
        .iter()
        .filter(|(_, _, _, _, fmt)| *fmt == OutputFormat::Markdown)
        .map(|(k, _, t, _, _)| (k.clone(), *t))
        .collect();
    let mut pdf_tf1_plaintext: Vec<(String, f64)> = pdf_metrics
        .iter()
        .filter(|(_, _, _, _, fmt)| *fmt == OutputFormat::Plaintext)
        .map(|(k, _, t, _, _)| (k.clone(), *t))
        .collect();
    let mut pdf_sf1_markdown: Vec<(String, f64)> = pdf_metrics
        .iter()
        .filter(|(_, _, _, _, fmt)| *fmt == OutputFormat::Markdown)
        .map(|(k, _, _, s, _)| (k.clone(), *s))
        .collect();

    let pdf_quality_ranking_markdown = build_ranking(&mut pdf_qual_markdown);
    let pdf_quality_ranking_plaintext = build_ranking(&mut pdf_qual_plaintext);
    let pdf_tf1_ranking_markdown = build_ranking(&mut pdf_tf1_markdown);
    let pdf_tf1_ranking_plaintext = build_ranking(&mut pdf_tf1_plaintext);
    let pdf_sf1_ranking_markdown = build_ranking(&mut pdf_sf1_markdown);

    // Higher pages/sec is better, so the shared descending `build_ranking` closure applies as-is.
    let pages_per_sec_ranking = build_ranking(&mut pages_per_sec_metrics);

    // Lower CPU-seconds is better: sort ascending, with the best (lowest) value as the `1.0`
    // relative baseline — mirrors `memory_ranking`'s ascending treatment above.
    cpu_seconds_metrics.retain(|(_, v)| v.is_finite());
    cpu_seconds_metrics.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let baseline_cpu_seconds = cpu_seconds_metrics.first().map(|(_, v)| *v).unwrap_or(1.0);
    let cpu_seconds_ranking: Vec<RankedFramework> = cpu_seconds_metrics
        .iter()
        .enumerate()
        .map(|(i, (k, v))| RankedFramework {
            framework_mode: k.clone(),
            rank: i + 1,
            value: *v,
            relative: if baseline_cpu_seconds > 0.0 {
                *v / baseline_cpu_seconds
            } else {
                1.0
            },
        })
        .collect();

    let pareto_frontier = build_pareto_frontier(by_framework_mode);

    ComparisonData {
        throughput_ranking,
        memory_ranking,
        quality_ranking_markdown,
        quality_ranking_plaintext,
        pdf_quality_ranking_markdown,
        pdf_quality_ranking_plaintext,
        pdf_tf1_ranking_markdown,
        pdf_tf1_ranking_plaintext,
        pdf_sf1_ranking_markdown,
        pages_per_sec_ranking,
        cpu_seconds_ranking,
        deltas_vs_baseline,
        pareto_frontier,
    }
}

/// Build the non-dominated (pages/sec ↑, SF1 ↑, peak-RSS ↓) frontier across markdown frameworks.
///
/// See [`ParetoPoint`] for the dominance rule and eligibility criteria (markdown output format,
/// a defined SF1 term, and at least one pages/sec observation).
fn build_pareto_frontier(by_framework_mode: &HashMap<String, FrameworkModeAggregation>) -> Vec<ParetoPoint> {
    let candidates: Vec<ParetoPoint> = by_framework_mode
        .iter()
        .filter(|(_, agg)| agg.output_format == OutputFormat::Markdown)
        .filter_map(|(key, agg)| {
            let performance = agg
                .overall_performance
                .as_ref()
                .filter(|performance| performance.performance_sample_count > 0)?;
            let pages_per_sec = performance.pages_per_sec.as_ref()?.p50;
            let sf1 = performance.quality.as_ref()?.f1_layout_p50?;
            let peak_memory_mb = performance.memory.p50;
            if !pages_per_sec.is_finite() || !sf1.is_finite() || !peak_memory_mb.is_finite() {
                return None;
            }
            Some(ParetoPoint {
                framework_mode: key.clone(),
                pages_per_sec,
                sf1,
                peak_memory_mb,
            })
        })
        .collect();

    candidates
        .iter()
        .filter(|candidate| {
            !candidates.iter().any(|other| {
                other.framework_mode != candidate.framework_mode
                    && other.pages_per_sec >= candidate.pages_per_sec
                    && other.sf1 >= candidate.sf1
                    && other.peak_memory_mb <= candidate.peak_memory_mb
                    && (other.pages_per_sec > candidate.pages_per_sec
                        || other.sf1 > candidate.sf1
                        || other.peak_memory_mb < candidate.peak_memory_mb)
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_load::SystemLoad;
    use crate::types::{ErrorKind, FrameworkCapabilities, OcrStatus, PdfMetadata, PerformanceMetrics, QualityMetrics};
    use std::path::PathBuf;
    use std::time::Duration;

    fn create_test_result(
        framework: &str,
        file_ext: &str,
        ocr_status: OcrStatus,
        duration_ms: u64,
        throughput_bps: f64,
        memory_bytes: u64,
    ) -> BenchmarkResult {
        BenchmarkResult {
            framework: framework.to_string(),
            file_path: PathBuf::from(format!("test.{}", file_ext)),
            file_size: 1024,
            success: true,
            error_message: None,
            error_kind: ErrorKind::None,
            duration: Duration::from_millis(duration_ms),
            extraction_duration: None,
            subprocess_overhead: None,
            metrics: PerformanceMetrics {
                baseline_memory_bytes: 0,
                peak_memory_bytes: memory_bytes,
                peak_memory_delta_bytes: memory_bytes,
                avg_cpu_percent: 50.0,
                cpu_seconds: 50.0,
                throughput_bytes_per_sec: throughput_bps,
                p50_memory_bytes: memory_bytes,
                p95_memory_bytes: memory_bytes,
                p99_memory_bytes: memory_bytes,
            },
            quality: None,
            iterations: vec![],
            statistics: None,
            cold_start_duration: Some(Duration::from_millis(500)),
            file_extension: file_ext.to_string(),
            framework_capabilities: FrameworkCapabilities::default(),
            pdf_metadata: None,
            ocr_status,
            output_format: OutputFormat::Markdown,
            extracted_text: None,
            system_load: None,
        }
    }

    #[test]
    fn test_extract_framework_and_mode() {
        assert_eq!(
            extract_framework_and_mode("xberg-markdown-baseline"),
            ("xberg-markdown-baseline", "single")
        );
        assert_eq!(
            extract_framework_and_mode("xberg-plaintext-paddle-ocr"),
            ("xberg-plaintext-paddle-ocr", "single")
        );
        assert_eq!(
            extract_framework_and_mode("xberg-markdown-baseline-batch"),
            ("xberg-markdown-baseline", "batch")
        );

        assert_eq!(extract_framework_and_mode("xberg-sync"), ("xberg", "single"));
        assert_eq!(extract_framework_and_mode("xberg-async"), ("xberg", "single"));

        assert_eq!(extract_framework_and_mode("xberg-batch"), ("xberg", "batch"));
        assert_eq!(extract_framework_and_mode("python-batch"), ("python", "batch"));

        assert_eq!(extract_framework_and_mode("xberg"), ("xberg", "single"));
        assert_eq!(extract_framework_and_mode("docling"), ("docling", "single"));
    }

    #[test]
    fn test_make_aggregate_key_xberg_family() {
        assert_eq!(
            make_aggregate_key("xberg-markdown-baseline", OutputFormat::Markdown, "single"),
            "xberg-markdown-baseline:single"
        );
        assert_eq!(
            make_aggregate_key("xberg-plaintext-layout", OutputFormat::Plaintext, "batch"),
            "xberg-plaintext-layout:batch"
        );
    }

    #[test]
    fn test_make_aggregate_key_competitors() {
        assert_eq!(
            make_aggregate_key("docling", OutputFormat::Markdown, "single"),
            "docling:markdown:single"
        );
        assert_eq!(
            make_aggregate_key("unstructured", OutputFormat::Plaintext, "batch"),
            "unstructured:plaintext:batch"
        );
    }

    #[test]
    fn test_aggregate_new_format_xberg_key_shape() {
        let results = vec![
            create_test_result(
                "xberg-markdown-baseline",
                "pdf",
                OcrStatus::NotUsed,
                100,
                1_000_000.0,
                10_000_000,
            ),
            create_test_result(
                "xberg-markdown-baseline-batch",
                "pdf",
                OcrStatus::NotUsed,
                80,
                1_000_000.0,
                10_000_000,
            ),
        ];

        let aggregated = aggregate_new_format(&results);

        assert_eq!(aggregated.by_framework_mode.len(), 2);
        assert!(
            aggregated
                .by_framework_mode
                .contains_key("xberg-markdown-baseline:single")
        );
        assert!(
            aggregated
                .by_framework_mode
                .contains_key("xberg-markdown-baseline:batch")
        );

        let single_agg = &aggregated.by_framework_mode["xberg-markdown-baseline:single"];
        assert_eq!(single_agg.framework, "xberg-markdown-baseline");
        assert_eq!(single_agg.mode, "single");
    }

    #[test]
    fn test_percentile_r7() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile_r7(&values, 0.0), 1.0);
        assert_eq!(percentile_r7(&values, 0.5), 3.0);
        assert_eq!(percentile_r7(&values, 1.0), 5.0);
        assert_eq!(percentile_r7(&[], 0.5), 0.0);
    }

    #[test]
    fn test_aggregate_new_format() {
        let results = vec![
            create_test_result("xberg-sync", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000),
            create_test_result("xberg-sync", "pdf", OcrStatus::Used, 200, 500_000.0, 20_000_000),
            create_test_result("xberg-batch", "docx", OcrStatus::NotUsed, 150, 750_000.0, 15_000_000),
        ];

        let aggregated = aggregate_new_format(&results);

        assert_eq!(aggregated.by_framework_mode.len(), 2);
        assert!(aggregated.by_framework_mode.contains_key("xberg:markdown:single"));
        assert!(aggregated.by_framework_mode.contains_key("xberg:markdown:batch"));

        let single_agg = &aggregated.by_framework_mode["xberg:markdown:single"];
        assert_eq!(single_agg.framework, "xberg");
        assert_eq!(single_agg.mode, "single");
        assert!(single_agg.cold_start.is_some());

        let pdf_agg = &single_agg.by_file_type["pdf"];
        assert!(pdf_agg.no_ocr.is_some());
        assert!(pdf_agg.with_ocr.is_some());

        assert_eq!(pdf_agg.no_ocr.as_ref().unwrap().successful_sample_count, 1);
        assert_eq!(pdf_agg.with_ocr.as_ref().unwrap().successful_sample_count, 1);
    }

    #[test]
    fn test_calculate_percentiles() {
        let results = [
            create_test_result("xberg", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000),
            create_test_result("xberg", "pdf", OcrStatus::NotUsed, 200, 2_000_000.0, 20_000_000),
            create_test_result("xberg", "pdf", OcrStatus::NotUsed, 300, 3_000_000.0, 30_000_000),
        ];

        let refs: Vec<&BenchmarkResult> = results.iter().collect();
        let percentiles = calculate_percentiles(&refs);

        assert_eq!(percentiles.successful_sample_count, 3);
        assert_eq!(percentiles.total_sample_count, 3);
        assert_eq!(percentiles.success_rate_percent, 100.0);
        assert!(percentiles.duration.p50 > 0.0);
        assert!(percentiles.throughput.p50 > 0.0);
        assert!(percentiles.memory.p50 > 0.0);
    }

    #[test]
    fn batch_process_metrics_are_sampled_once_while_item_durations_are_preserved() {
        let capability = crate::types::BatchCapability {
            entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: true,
        };
        let mut results = [
            create_test_result("xberg-batch", "pdf", OcrStatus::NotUsed, 100, 3_000_000.0, 10_000_000),
            create_test_result("xberg-batch", "pdf", OcrStatus::NotUsed, 900, 0.0, 90_000_000),
            create_test_result("xberg-batch", "pdf", OcrStatus::NotUsed, 1_700, 0.0, 170_000_000),
        ];
        for (index, result) in results.iter_mut().enumerate() {
            result.framework_capabilities.batch_support = true;
            result.framework_capabilities.batch_capability = Some(capability);
            result.framework_capabilities.batch_performance_sample = Some(index == 0);
            result.extraction_duration = Some(Duration::from_millis((index as u64 + 1) * 10));
        }

        let refs: Vec<&BenchmarkResult> = results.iter().collect();
        let percentiles = calculate_percentiles(&refs);

        assert_eq!(percentiles.successful_sample_count, 3);
        assert_eq!(percentiles.performance_sample_count, 1);
        assert_eq!(percentiles.duration.p50, 100.0);
        assert_eq!(percentiles.memory.p50, 10.0);
        assert_eq!(percentiles.throughput.p50, 3.0);
        assert_eq!(
            percentiles.extraction_duration.as_ref().map(|values| values.p50),
            Some(20.0)
        );
    }

    #[test]
    fn mixed_batch_buckets_retain_one_process_sample_independent_of_input_order() {
        let build_results = |reversed: bool| {
            let mut results = vec![
                create_test_result("xberg-batch", "pdf", OcrStatus::NotUsed, 100, 3_000_000.0, 10_000_000),
                create_test_result("xberg-batch", "docx", OcrStatus::Used, 100, 3_000_000.0, 10_000_000),
            ];
            if reversed {
                results.reverse();
            }
            let capability = crate::types::BatchCapability {
                entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
                timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
                per_item_timing: true,
            };
            for (index, result) in results.iter_mut().enumerate() {
                result.framework_capabilities.batch_support = true;
                result.framework_capabilities.batch_capability = Some(capability);
                result.framework_capabilities.batch_performance_sample = Some(index == 0);
                result.framework_capabilities.batch_sample_id = Some(format!("mixed-batch-{reversed}"));
            }
            results
        };

        for reversed in [false, true] {
            let aggregated = aggregate_new_format(&build_results(reversed));
            let framework = &aggregated.by_framework_mode["xberg:markdown:batch"];
            let overall = framework.overall_performance.as_ref().expect("overall process metrics");
            let pdf = framework.by_file_type["pdf"]
                .no_ocr
                .as_ref()
                .expect("PDF no-OCR metrics");
            let docx = framework.by_file_type["docx"]
                .with_ocr
                .as_ref()
                .expect("DOCX OCR metrics");

            assert_eq!(overall.performance_sample_count, 1);
            assert_eq!(pdf.performance_sample_count, 1);
            assert_eq!(docx.performance_sample_count, 1);
            assert_eq!(overall.throughput.p50, 3.0);
            assert_eq!(pdf.throughput.p50, 3.0);
            assert_eq!(docx.throughput.p50, 3.0);
            assert_eq!(aggregated.comparison.throughput_ranking[0].value, 3.0);
        }
    }

    #[test]
    fn repeated_identical_semantic_batches_remain_independent_process_samples() {
        let capability = crate::types::BatchCapability {
            entry_point: crate::types::BatchEntryPoint::DoclingConvertAll,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: false,
        };
        let mut results = Vec::new();
        for (sample_id, duration, throughput, memory) in [
            ("invocation-1", 100, 1_000_000.0, 10_000_000),
            ("invocation-2", 300, 3_000_000.0, 30_000_000),
        ] {
            for sibling in 0..2 {
                let mut result =
                    create_test_result("docling-batch", "pdf", OcrStatus::NotUsed, duration, throughput, memory);
                result.framework_capabilities.batch_support = true;
                result.framework_capabilities.batch_capability = Some(capability);
                result.framework_capabilities.batch_performance_sample = Some(sibling == 0);
                result.framework_capabilities.batch_sample_id = Some(sample_id.to_string());
                results.push(result);
            }
        }

        let aggregated = aggregate_new_format(&results);
        let overall = aggregated.by_framework_mode["docling:markdown:batch"]
            .overall_performance
            .as_ref()
            .expect("overall process metrics");

        assert_eq!(overall.performance_sample_count, 2);
        assert_eq!(overall.duration.p50, 200.0);
        assert_eq!(overall.throughput.p50, 2.0);
        assert_eq!(overall.memory.p50, 20.0);
    }

    #[test]
    fn batch_capable_single_zero_throughput_is_an_explicit_process_sample() {
        let mut result = create_test_result("docling", "pdf", OcrStatus::NotUsed, 100, 0.0, 10_000_000);
        result.framework_capabilities.batch_support = true;
        result.framework_capabilities.batch_capability = Some(crate::types::BatchCapability {
            entry_point: crate::types::BatchEntryPoint::DoclingConvertAll,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: false,
        });
        result.framework_capabilities.batch_performance_sample = Some(true);

        let percentiles = calculate_percentiles(&[&result]);

        assert_eq!(percentiles.performance_sample_count, 1);
        assert_eq!(percentiles.duration.p50, 100.0);
        assert_eq!(percentiles.memory.p50, 10.0);
    }

    #[test]
    fn legacy_batch_rows_fall_back_to_the_positive_throughput_anchor() {
        let capability = crate::types::BatchCapability {
            entry_point: crate::types::BatchEntryPoint::DoclingConvertAll,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: false,
        };
        let mut anchor = create_test_result("docling", "pdf", OcrStatus::NotUsed, 100, 3_000_000.0, 10_000_000);
        let mut sibling = create_test_result("docling", "pdf", OcrStatus::NotUsed, 100, 0.0, 10_000_000);
        for result in [&mut anchor, &mut sibling] {
            result.framework_capabilities.batch_support = true;
            result.framework_capabilities.batch_capability = Some(capability);
        }

        let percentiles = calculate_percentiles(&[&anchor, &sibling]);

        assert_eq!(percentiles.performance_sample_count, 1);
        assert_eq!(percentiles.throughput.p50, 3.0);
    }

    #[test]
    fn test_aggregate_cold_starts() {
        let results = [
            create_test_result("xberg", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000),
            create_test_result("xberg", "pdf", OcrStatus::NotUsed, 200, 2_000_000.0, 20_000_000),
        ];

        let refs: Vec<&BenchmarkResult> = results.iter().collect();
        let cold_starts = aggregate_cold_starts(&refs);

        assert!(cold_starts.is_some());
        let cold_starts = cold_starts.unwrap();
        assert_eq!(cold_starts.sample_count, 2);
        assert!(cold_starts.p50_ms > 0.0);
    }

    #[test]
    fn test_ocr_unknown_is_not_mislabeled() {
        let results = vec![BenchmarkResult {
            framework: "test-framework".to_string(),
            file_path: PathBuf::from("/tmp/test1.pdf"),
            file_size: 1024,
            success: true,
            error_message: None,
            error_kind: ErrorKind::None,
            duration: Duration::from_millis(100),
            extraction_duration: None,
            subprocess_overhead: None,
            metrics: PerformanceMetrics {
                baseline_memory_bytes: 0,
                peak_memory_bytes: 10_000_000,
                peak_memory_delta_bytes: 10_000_000,
                avg_cpu_percent: 50.0,
                cpu_seconds: 50.0,
                throughput_bytes_per_sec: 10_240.0,
                p50_memory_bytes: 8_000_000,
                p95_memory_bytes: 9_500_000,
                p99_memory_bytes: 9_900_000,
            },
            quality: None,
            iterations: vec![],
            statistics: None,
            cold_start_duration: Some(Duration::from_millis(200)),
            file_extension: "pdf".to_string(),
            framework_capabilities: Default::default(),
            pdf_metadata: None,
            ocr_status: OcrStatus::Unknown,
            extracted_text: None,
            system_load: None,
            output_format: OutputFormat::Markdown,
        }];

        let aggregated = aggregate_new_format(&results);

        let framework_mode = aggregated
            .by_framework_mode
            .get("test-framework:markdown:single")
            .unwrap();
        let file_type = framework_mode.by_file_type.get("pdf").unwrap();
        assert!(file_type.no_ocr.is_none());
        assert!(file_type.with_ocr.is_none());
    }

    #[test]
    fn test_failed_results_excluded_from_percentiles() {
        let results = vec![
            BenchmarkResult {
                framework: "test-framework".to_string(),
                file_path: PathBuf::from("/tmp/test1.pdf"),
                file_size: 1024,
                success: true,
                error_message: None,
                error_kind: ErrorKind::None,
                duration: Duration::from_millis(100),
                extraction_duration: None,
                subprocess_overhead: None,
                metrics: PerformanceMetrics {
                    baseline_memory_bytes: 0,
                    peak_memory_bytes: 10_000_000,
                    peak_memory_delta_bytes: 10_000_000,
                    avg_cpu_percent: 50.0,
                    cpu_seconds: 50.0,
                    throughput_bytes_per_sec: 10_240.0,
                    p50_memory_bytes: 8_000_000,
                    p95_memory_bytes: 9_500_000,
                    p99_memory_bytes: 9_900_000,
                },
                quality: None,
                iterations: vec![],
                statistics: None,
                cold_start_duration: None,
                file_extension: "pdf".to_string(),
                framework_capabilities: Default::default(),
                pdf_metadata: None,
                ocr_status: OcrStatus::NotUsed,
                extracted_text: None,
                system_load: None,
                output_format: OutputFormat::Markdown,
            },
            BenchmarkResult {
                framework: "test-framework".to_string(),
                file_path: PathBuf::from("/tmp/test2.pdf"),
                file_size: 2048,
                success: false,
                error_message: Some("Test error".to_string()),
                error_kind: ErrorKind::HarnessError,
                duration: Duration::from_secs(0),
                extraction_duration: None,
                subprocess_overhead: None,
                metrics: PerformanceMetrics {
                    baseline_memory_bytes: 0,
                    peak_memory_bytes: 0,
                    peak_memory_delta_bytes: 0,
                    avg_cpu_percent: 0.0,
                    cpu_seconds: 0.0,
                    throughput_bytes_per_sec: 0.0,
                    p50_memory_bytes: 0,
                    p95_memory_bytes: 0,
                    p99_memory_bytes: 0,
                },
                quality: None,
                iterations: vec![],
                statistics: None,
                cold_start_duration: None,
                file_extension: "pdf".to_string(),
                framework_capabilities: Default::default(),
                pdf_metadata: None,
                ocr_status: OcrStatus::NotUsed,
                extracted_text: None,
                system_load: None,
                output_format: OutputFormat::Markdown,
            },
        ];

        let aggregated = aggregate_new_format(&results);

        let framework_mode = aggregated
            .by_framework_mode
            .get("test-framework:markdown:single")
            .unwrap();
        let file_type = framework_mode.by_file_type.get("pdf").unwrap();
        let no_ocr = file_type.no_ocr.as_ref().unwrap();

        assert_eq!(no_ocr.successful_sample_count, 1);
        assert_eq!(no_ocr.total_sample_count, 2);
        assert_eq!(no_ocr.success_rate_percent, 50.0);
        assert_eq!(no_ocr.duration.p50, 100.0);
    }

    #[test]
    fn test_empty_input() {
        let results: Vec<BenchmarkResult> = vec![];
        let aggregated = aggregate_new_format(&results);

        assert_eq!(aggregated.by_framework_mode.len(), 0);
        assert_eq!(aggregated.metadata.total_results, 0);
    }

    #[test]
    fn test_percentile_interpolation() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let p95 = percentile_r7(&sorted, 0.95);

        assert!((p95 - 4.8).abs() < 0.01);
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_all_present() {
        let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result1.extraction_duration = Some(Duration::from_millis(80));

        let mut result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
        result2.extraction_duration = Some(Duration::from_millis(120));

        let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
        result3.extraction_duration = Some(Duration::from_millis(160));

        let refs = vec![&result1, &result2, &result3];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_some());
        let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
        assert!((ext_dur.p50 - 120.0).abs() < 0.1);
        assert!(ext_dur.p95 > 120.0);
        assert!(ext_dur.p95 <= 160.0);
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_all_none() {
        let result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        let result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
        let result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);

        let refs = vec![&result1, &result2, &result3];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_none());
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_mixed() {
        let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result1.extraction_duration = Some(Duration::from_millis(80));

        let result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);

        let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
        result3.extraction_duration = Some(Duration::from_millis(160));

        let refs = vec![&result1, &result2, &result3];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_some());
        let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
        assert!((ext_dur.p50 - 120.0).abs() < 0.1);
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_filters_invalid() {
        let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result1.extraction_duration = Some(Duration::from_millis(80));

        let mut result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
        result2.extraction_duration = Some(Duration::from_millis(120));

        let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
        result3.extraction_duration = Some(Duration::from_millis(160));

        let refs = vec![&result1, &result2, &result3];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_some());
        let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
        assert!(ext_dur.p50.is_finite());
        assert!(!ext_dur.p50.is_nan());
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_with_failed_results() {
        let mut result1 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result1.extraction_duration = Some(Duration::from_millis(80));

        let mut result2_failed = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 0, 0.0, 0);
        result2_failed.success = false;
        result2_failed.error_message = Some("Failed".to_string());
        result2_failed.extraction_duration = Some(Duration::from_millis(50));

        let mut result3 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 200, 1_000_000.0, 10_000_000);
        result3.extraction_duration = Some(Duration::from_millis(160));

        let refs = vec![&result1, &result2_failed, &result3];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_some());
        let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
        assert_eq!(percentiles.successful_sample_count, 2);
        assert_eq!(percentiles.total_sample_count, 3);
        assert!((ext_dur.p50 - 120.0).abs() < 0.1);
    }

    #[test]
    fn test_aggregate_by_ocr_status_extraction_duration() {
        let mut result_no_ocr_1 =
            create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result_no_ocr_1.extraction_duration = Some(Duration::from_millis(80));

        let mut result_no_ocr_2 =
            create_test_result("framework1", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
        result_no_ocr_2.extraction_duration = Some(Duration::from_millis(120));

        let mut result_with_ocr = create_test_result("framework1", "pdf", OcrStatus::Used, 300, 500_000.0, 20_000_000);
        result_with_ocr.extraction_duration = Some(Duration::from_millis(250));

        let refs = vec![&result_no_ocr_1, &result_no_ocr_2, &result_with_ocr];
        let (no_ocr, with_ocr) = aggregate_by_ocr_status(&refs);

        assert!(no_ocr.is_some());
        let no_ocr_perf = no_ocr.unwrap();
        assert!(no_ocr_perf.extraction_duration.is_some());
        assert_eq!(no_ocr_perf.extraction_duration.as_ref().unwrap().p50, 100.0);

        assert!(with_ocr.is_some());
        let with_ocr_perf = with_ocr.unwrap();
        assert!(with_ocr_perf.extraction_duration.is_some());
        assert_eq!(with_ocr_perf.extraction_duration.as_ref().unwrap().p50, 250.0);
    }

    #[test]
    fn unknown_pdf_ocr_status_is_excluded_from_ocr_cohorts() {
        let unknown = create_test_result("framework1", "pdf", OcrStatus::Unknown, 100, 1_000_000.0, 10_000_000);
        let refs = vec![&unknown];

        let (no_ocr, with_ocr) = aggregate_by_ocr_status(&refs);

        assert!(no_ocr.is_none());
        assert!(with_ocr.is_none());
    }

    #[test]
    fn test_aggregate_new_format_extraction_duration_preserved() {
        let mut result1 = create_test_result("xberg-sync", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result1.extraction_duration = Some(Duration::from_millis(80));

        let mut result2 = create_test_result("xberg-sync", "pdf", OcrStatus::NotUsed, 150, 1_000_000.0, 10_000_000);
        result2.extraction_duration = Some(Duration::from_millis(120));

        let results = vec![result1, result2];
        let aggregated = aggregate_new_format(&results);

        let framework_mode = aggregated.by_framework_mode.get("xberg:markdown:single").unwrap();
        let pdf_stats = framework_mode.by_file_type.get("pdf").unwrap();
        let no_ocr = pdf_stats.no_ocr.as_ref().unwrap();

        assert!(no_ocr.extraction_duration.is_some());
        let ext_dur = no_ocr.extraction_duration.as_ref().unwrap();
        assert!((ext_dur.p50 - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_single_value() {
        let mut result = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result.extraction_duration = Some(Duration::from_millis(80));

        let refs = vec![&result];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_some());
        let ext_dur = percentiles.extraction_duration.as_ref().unwrap();
        assert_eq!(ext_dur.p50, 80.0);
        assert_eq!(ext_dur.p95, 80.0);
        assert_eq!(ext_dur.p99, 80.0);
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_large_dataset() {
        let mut results = vec![];
        for i in 1..=100 {
            let mut result =
                create_test_result("framework1", "pdf", OcrStatus::NotUsed, i * 10, 1_000_000.0, 10_000_000);
            result.extraction_duration = Some(Duration::from_millis(i * 8));
            results.push(result);
        }

        let refs: Vec<&BenchmarkResult> = results.iter().collect();
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_some());
        let ext_dur = percentiles.extraction_duration.as_ref().unwrap();

        assert!(ext_dur.p50 >= 400.0 && ext_dur.p50 <= 410.0);

        assert!(ext_dur.p95 > ext_dur.p50);

        assert!(ext_dur.p99 > ext_dur.p95);
    }

    /// Regression test for the plaintext/markdown quality-ranking pooling bug.
    ///
    /// A plaintext-only framework (scored with no structural/SF1 term) must never be pooled
    /// into a markdown (layout-inclusive) quality ranking alongside frameworks that carry a
    /// structural penalty. See module-level docs ("Output format support") for the contract.
    #[test]
    fn test_quality_ranking_never_pools_plaintext_into_markdown() {
        let mut markdown_result = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );
        markdown_result.output_format = OutputFormat::Markdown;
        markdown_result.quality = Some(crate::types::QualityMetrics {
            f1_score_text: 0.7,
            f1_score_numeric: 0.7,
            f1_score_layout: Some(0.5),
            quality_score: 0.5 * 0.7 + 0.2 * 0.7 + 0.3 * 0.5,
            missing_tokens: vec![],
            extra_tokens: vec![],
            correct: false,
        });

        // A plaintext-only competitor (e.g. Apache Tika): higher raw quality_score because it
        // never incurs the structural (SF1) penalty markdown frameworks carry. ~keep
        let mut plaintext_result =
            create_test_result("apache-tika", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        plaintext_result.output_format = OutputFormat::Plaintext;
        plaintext_result.quality = Some(crate::types::QualityMetrics {
            f1_score_text: 0.95,
            f1_score_numeric: 0.95,
            f1_score_layout: None,
            quality_score: 0.6 * 0.95 + 0.4 * 0.95,
            missing_tokens: vec![],
            extra_tokens: vec![],
            correct: false,
        });

        let results = vec![markdown_result, plaintext_result];
        let aggregated = aggregate_new_format(&results);

        let markdown_keys: std::collections::HashSet<&str> = aggregated
            .comparison
            .quality_ranking_markdown
            .iter()
            .map(|r| r.framework_mode.as_str())
            .collect();
        let plaintext_keys: std::collections::HashSet<&str> = aggregated
            .comparison
            .quality_ranking_plaintext
            .iter()
            .map(|r| r.framework_mode.as_str())
            .collect();

        assert!(
            !markdown_keys.iter().any(|k| k.contains("apache-tika")),
            "plaintext-only framework 'apache-tika' must never appear in the markdown \
             (layout-inclusive) quality ranking, found in: {:?}",
            markdown_keys
        );
        assert!(
            markdown_keys.iter().any(|k| k.contains("xberg-markdown-baseline")),
            "markdown framework should appear in the markdown quality ranking, found: {:?}",
            markdown_keys
        );
        assert!(
            plaintext_keys.iter().any(|k| k.contains("apache-tika")),
            "plaintext framework should appear in the plaintext quality ranking, found: {:?}",
            plaintext_keys
        );

        let pdf_markdown_keys: std::collections::HashSet<&str> = aggregated
            .comparison
            .pdf_quality_ranking_markdown
            .iter()
            .map(|r| r.framework_mode.as_str())
            .collect();
        assert!(
            !pdf_markdown_keys.iter().any(|k| k.contains("apache-tika")),
            "plaintext-only framework must never appear in pdf_quality_ranking_markdown, found: {:?}",
            pdf_markdown_keys
        );
    }

    #[test]
    fn test_calculate_percentiles_extraction_duration_no_extraction_some_failed() {
        let result1_failed = BenchmarkResult {
            framework: "test".to_string(),
            file_path: PathBuf::from("test1.pdf"),
            file_size: 1024,
            success: false,
            error_message: Some("Error".to_string()),
            error_kind: ErrorKind::HarnessError,
            duration: Duration::from_millis(0),
            extraction_duration: None,
            subprocess_overhead: None,
            metrics: PerformanceMetrics {
                baseline_memory_bytes: 0,
                peak_memory_bytes: 0,
                peak_memory_delta_bytes: 0,
                avg_cpu_percent: 0.0,
                cpu_seconds: 0.0,
                throughput_bytes_per_sec: 0.0,
                p50_memory_bytes: 0,
                p95_memory_bytes: 0,
                p99_memory_bytes: 0,
            },
            quality: None,
            iterations: vec![],
            statistics: None,
            cold_start_duration: None,
            file_extension: "pdf".to_string(),
            framework_capabilities: FrameworkCapabilities::default(),
            pdf_metadata: None,
            ocr_status: OcrStatus::NotUsed,
            extracted_text: None,
            system_load: None,
            output_format: OutputFormat::Markdown,
        };

        let result2 = create_test_result("framework1", "pdf", OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);

        let refs = vec![&result1_failed, &result2];
        let percentiles = calculate_percentiles(&refs);

        assert!(percentiles.extraction_duration.is_none());
        assert_eq!(percentiles.success_rate_percent, 50.0);
    }

    fn result_with_quality(framework: &str, file_ext: &str, quality_score: f64, success: bool) -> BenchmarkResult {
        let mut result = create_test_result(framework, file_ext, OcrStatus::NotUsed, 100, 1_000_000.0, 10_000_000);
        result.success = success;
        if success {
            result.quality = Some(crate::types::QualityMetrics {
                f1_score_text: quality_score,
                f1_score_numeric: quality_score,
                f1_score_layout: Some(quality_score),
                quality_score,
                missing_tokens: vec![],
                extra_tokens: vec![],
                correct: false,
            });
        } else {
            result.error_message = Some("extraction failed".to_string());
            result.error_kind = ErrorKind::FrameworkError;
        }
        result
    }

    /// Regression test for Bug A: the overall quality ranking must only compare frameworks on
    /// the file types they *all* attempted (shared corpus), not on whatever subset each
    /// framework happened to run.
    ///
    /// `framework-partial` only ever attempts `pdf`, scoring high (0.9) there. `framework-full`
    /// attempts `pdf` (scoring lower, 0.6) plus `json` (scoring very low, 0.1) — a file type
    /// `framework-partial` never touched. Before the fix, `framework-full`'s overall mean would
    /// be dragged down by `json` while `framework-partial` was judged on `pdf` alone, an
    /// apples-to-oranges comparison. After the fix, both are ranked on the shared corpus (`pdf`
    /// only), so `framework-partial` (0.9) correctly outranks `framework-full` (0.6) — and
    /// `framework-full`'s `json` score must not appear in the shared-corpus mean at all.
    #[test]
    fn test_quality_ranking_restricted_to_shared_corpus() {
        let results = vec![
            result_with_quality("framework-partial", "pdf", 0.9, true),
            result_with_quality("framework-full", "pdf", 0.6, true),
            result_with_quality("framework-full", "json", 0.1, true),
        ];

        let aggregated = aggregate_new_format(&results);
        let ranking = &aggregated.comparison.quality_ranking_markdown;

        let partial = ranking
            .iter()
            .find(|r| r.framework_mode.contains("framework-partial"))
            .expect("framework-partial should be present in the shared-corpus ranking");
        let full = ranking
            .iter()
            .find(|r| r.framework_mode.contains("framework-full"))
            .expect("framework-full should be present in the shared-corpus ranking");

        assert!(
            (partial.value - 0.9).abs() < 1e-9,
            "framework-partial's shared-corpus (pdf-only) mean should be 0.9, got {}",
            partial.value
        );
        assert!(
            (full.value - 0.6).abs() < 1e-9,
            "framework-full's shared-corpus mean must only reflect pdf (0.6), not be diluted by \
             its json-only score; got {}",
            full.value
        );
        assert_eq!(
            partial.rank, 1,
            "framework-partial (0.9) should outrank framework-full (0.6) on shared pdf corpus"
        );
        assert_eq!(full.rank, 2);
    }

    /// Regression test for Bug B: a framework that attempted a file type but failed on every
    /// sample must rank BELOW a framework that succeeded on that same file type, not be
    /// silently excluded from the comparison as if it had never run at all.
    ///
    /// `framework-ok` succeeds on all its `pdf` samples (quality 0.8). `framework-crashed`
    /// attempts the same `pdf` file type but fails on every sample (mirrors docling failing
    /// 100% of a PDF corpus). Before the fix, `framework-crashed`'s zero-success pdf bucket was
    /// dropped entirely, so it would not appear in the ranking (or would be silently absent from
    /// the comparison) despite having completely failed. After the fix, `framework-crashed`
    /// contributes a quality value of 0.0 for that bucket and must rank strictly below
    /// `framework-ok`.
    #[test]
    fn test_fully_failed_bucket_ranks_below_succeeding_framework() {
        let results = vec![
            result_with_quality("framework-ok", "pdf", 0.8, true),
            result_with_quality("framework-ok", "pdf", 0.8, true),
            result_with_quality("framework-crashed", "pdf", 0.0, false),
            result_with_quality("framework-crashed", "pdf", 0.0, false),
        ];

        let aggregated = aggregate_new_format(&results);
        let ranking = &aggregated.comparison.quality_ranking_markdown;

        let ok = ranking
            .iter()
            .find(|r| r.framework_mode.contains("framework-ok"))
            .expect("framework-ok should be present");
        let crashed = ranking
            .iter()
            .find(|r| r.framework_mode.contains("framework-crashed"))
            .expect(
                "framework-crashed must appear in the ranking (as a 0.0 contribution), not be \
                 silently dropped for having zero successes",
            );

        assert!(
            crashed.value < ok.value,
            "a fully-failed framework must score below a succeeding one: crashed={}, ok={}",
            crashed.value,
            ok.value
        );
        assert!(
            (crashed.value - 0.0).abs() < 1e-9,
            "fully-failed bucket should contribute 0.0, got {}",
            crashed.value
        );
        assert!(ok.rank < crashed.rank, "framework-ok must outrank framework-crashed");

        let pdf_ranking = &aggregated.comparison.pdf_quality_ranking_markdown;
        let pdf_crashed = pdf_ranking
            .iter()
            .find(|r| r.framework_mode.contains("framework-crashed"))
            .expect("framework-crashed must appear in pdf_quality_ranking_markdown as a 0.0 entry");
        let pdf_ok = pdf_ranking
            .iter()
            .find(|r| r.framework_mode.contains("framework-ok"))
            .expect("framework-ok must appear in pdf_quality_ranking_markdown");
        assert!(pdf_ok.rank < pdf_crashed.rank);
    }

    fn pdf_metadata_with_page_count(page_count: u32) -> PdfMetadata {
        PdfMetadata {
            has_text_layer: true,
            detection_method: "pdftotext".to_string(),
            page_count: Some(page_count),
            ocr_enabled: false,
            text_quality_score: None,
        }
    }

    #[test]
    fn pages_per_sec_percentile_from_single_file_pdf_metadata() {
        let mut result = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            2_000,
            1_000_000.0,
            10_000_000,
        );
        result.pdf_metadata = Some(pdf_metadata_with_page_count(20));

        let percentiles = calculate_percentiles(&[&result]);

        // 20 pages / 2.0 seconds = 10.0 pages/sec.
        let pages_per_sec = percentiles.pages_per_sec.expect("pages_per_sec must be populated");
        assert_eq!(pages_per_sec.p50, 10.0);
        assert_eq!(pages_per_sec.p95, 10.0);
        assert_eq!(pages_per_sec.p99, 10.0);
    }

    #[test]
    fn pages_per_sec_is_none_without_any_page_count_data() {
        let result = create_test_result(
            "xberg-markdown-baseline",
            "docx",
            OcrStatus::NotUsed,
            1_000,
            1_000_000.0,
            10_000_000,
        );

        let percentiles = calculate_percentiles(&[&result]);

        assert!(percentiles.pages_per_sec.is_none());
    }

    #[test]
    fn pages_per_sec_sums_page_counts_across_one_batch_invocation() {
        let capability = crate::types::BatchCapability {
            entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: true,
        };

        let mut doc_a = create_test_result(
            "xberg-markdown-baseline-batch",
            "pdf",
            OcrStatus::NotUsed,
            4_000,
            1_000_000.0,
            10_000_000,
        );
        doc_a.framework_capabilities.batch_support = true;
        doc_a.framework_capabilities.batch_capability = Some(capability);
        doc_a.framework_capabilities.batch_performance_sample = Some(true);
        doc_a.framework_capabilities.batch_sample_id = Some("batch-1".to_string());
        doc_a.pdf_metadata = Some(pdf_metadata_with_page_count(12));

        let mut doc_b = create_test_result(
            "xberg-markdown-baseline-batch",
            "pdf",
            OcrStatus::NotUsed,
            4_000,
            1_000_000.0,
            10_000_000,
        );
        doc_b.framework_capabilities.batch_support = true;
        doc_b.framework_capabilities.batch_capability = Some(capability);
        doc_b.framework_capabilities.batch_performance_sample = Some(false);
        doc_b.framework_capabilities.batch_sample_id = Some("batch-1".to_string());
        doc_b.pdf_metadata = Some(pdf_metadata_with_page_count(8));

        let percentiles = calculate_percentiles(&[&doc_a, &doc_b]);

        // Total 20 pages across the one batch invocation, over its shared 4-second makespan.
        let pages_per_sec = percentiles
            .pages_per_sec
            .expect("pages_per_sec must be populated for the batch");
        assert_eq!(pages_per_sec.p50, 5.0);
        assert_eq!(percentiles.performance_sample_count, 1);
    }

    #[test]
    fn cpu_seconds_percentile_aggregates_from_performance_samples() {
        let mut r1 = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );
        r1.metrics.cpu_seconds = 1.0;
        let mut r2 = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );
        r2.metrics.cpu_seconds = 2.0;
        let mut r3 = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );
        r3.metrics.cpu_seconds = 3.0;

        let percentiles = calculate_percentiles(&[&r1, &r2, &r3]);

        assert_eq!(percentiles.cpu_seconds.p50, 2.0);
    }

    #[test]
    fn batch_size_is_one_for_single_file_mode() {
        let result = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );

        let percentiles = calculate_percentiles(&[&result]);

        assert_eq!(percentiles.batch_size, Some(1));
    }

    #[test]
    fn batch_size_reflects_documents_per_batch_invocation() {
        let capability = crate::types::BatchCapability {
            entry_point: crate::types::BatchEntryPoint::XbergCliExtractBatch,
            timing_scope: crate::types::BatchTimingScope::ColdEndToEndSubprocess,
            per_item_timing: true,
        };
        let mut results = [
            create_test_result(
                "xberg-markdown-baseline-batch",
                "pdf",
                OcrStatus::NotUsed,
                100,
                1_000_000.0,
                10_000_000,
            ),
            create_test_result(
                "xberg-markdown-baseline-batch",
                "pdf",
                OcrStatus::NotUsed,
                100,
                1_000_000.0,
                10_000_000,
            ),
            create_test_result(
                "xberg-markdown-baseline-batch",
                "pdf",
                OcrStatus::NotUsed,
                100,
                1_000_000.0,
                10_000_000,
            ),
            create_test_result(
                "xberg-markdown-baseline-batch",
                "pdf",
                OcrStatus::NotUsed,
                100,
                1_000_000.0,
                10_000_000,
            ),
        ];
        for (index, result) in results.iter_mut().enumerate() {
            result.framework_capabilities.batch_support = true;
            result.framework_capabilities.batch_capability = Some(capability);
            result.framework_capabilities.batch_performance_sample = Some(index == 0);
            result.framework_capabilities.batch_sample_id = Some("batch-of-4".to_string());
        }

        let refs: Vec<&BenchmarkResult> = results.iter().collect();
        let percentiles = calculate_percentiles(&refs);

        assert_eq!(percentiles.performance_sample_count, 1);
        assert_eq!(percentiles.batch_size, Some(4));
    }

    #[test]
    fn system_load_aggregates_contention_across_results() {
        let mut idle = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );
        idle.system_load = Some(SystemLoad {
            load_avg_1m: 1.0,
            load_avg_5m: 1.0,
            load_avg_15m: 1.0,
            logical_cores: 10,
            physical_cores: 10,
        });

        let mut busy = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );
        busy.system_load = Some(SystemLoad {
            load_avg_1m: 12.0,
            load_avg_5m: 12.0,
            load_avg_15m: 12.0,
            logical_cores: 10,
            physical_cores: 10,
        });

        let percentiles = calculate_percentiles(&[&idle, &busy]);

        let system_load = percentiles.system_load.expect("system_load must be populated");
        assert_eq!(system_load.total_sample_count, 2);
        assert_eq!(
            system_load.contended_sample_count, 1,
            "only the busy sample (load_per_core 1.2 > 0.7 threshold) should count as contended"
        );
        // R7 median of [0.1, 1.2] (load_avg_1m / logical_cores, sorted).
        assert!((system_load.load_per_core_p50 - 0.65).abs() < 1e-9);
    }

    #[test]
    fn system_load_is_none_without_any_captured_snapshot() {
        let result = create_test_result(
            "xberg-markdown-baseline",
            "pdf",
            OcrStatus::NotUsed,
            100,
            1_000_000.0,
            10_000_000,
        );

        let percentiles = calculate_percentiles(&[&result]);

        assert!(percentiles.system_load.is_none());
    }

    fn markdown_pdf_result(
        framework: &str,
        duration_ms: u64,
        memory_bytes: u64,
        page_count: u32,
        f1_layout: f64,
    ) -> BenchmarkResult {
        let mut result = create_test_result(
            framework,
            "pdf",
            OcrStatus::NotUsed,
            duration_ms,
            1_000_000.0,
            memory_bytes,
        );
        result.pdf_metadata = Some(pdf_metadata_with_page_count(page_count));
        result.quality = Some(QualityMetrics {
            f1_score_text: 0.9,
            f1_score_numeric: 0.9,
            f1_score_layout: Some(f1_layout),
            quality_score: 0.9,
            missing_tokens: vec![],
            extra_tokens: vec![],
            correct: false,
        });
        result
    }

    /// `framework-fast` dominates `framework-dominated` on all three Pareto axes (higher
    /// pages/sec, higher SF1, lower peak-RSS), so `framework-dominated` must be excluded from
    /// the frontier. `framework-balanced` trades pages/sec for SF1 and memory against
    /// `framework-fast` (neither dominates the other), so both survive.
    #[test]
    fn pareto_frontier_excludes_the_dominated_candidate() {
        let results = vec![
            markdown_pdf_result("framework-fast", 1_000, 500_000_000, 100, 0.70),
            markdown_pdf_result("framework-balanced", 2_000, 200_000_000, 100, 0.90),
            markdown_pdf_result("framework-dominated", 4_000, 900_000_000, 100, 0.60),
        ];

        let aggregated = aggregate_new_format(&results);
        let frontier = &aggregated.comparison.pareto_frontier;
        let frontier_keys: std::collections::HashSet<&str> =
            frontier.iter().map(|point| point.framework_mode.as_str()).collect();

        assert_eq!(
            frontier.len(),
            2,
            "expected exactly fast + balanced on the frontier: {frontier:?}"
        );
        assert!(frontier_keys.iter().any(|k| k.contains("framework-fast")));
        assert!(frontier_keys.iter().any(|k| k.contains("framework-balanced")));
        assert!(
            !frontier_keys.iter().any(|k| k.contains("framework-dominated")),
            "framework-dominated must be excluded: framework-fast strictly dominates it on all three axes, got {frontier:?}"
        );

        let fast_point = frontier
            .iter()
            .find(|point| point.framework_mode.contains("framework-fast"))
            .expect("fast point present");
        assert_eq!(fast_point.pages_per_sec, 100.0);
        assert_eq!(fast_point.sf1, 0.70);
        assert_eq!(fast_point.peak_memory_mb, 500.0);
    }

    #[test]
    fn pareto_frontier_excludes_plaintext_frameworks() {
        let mut plaintext = markdown_pdf_result("framework-plaintext", 1_000, 100_000_000, 100, 0.95);
        plaintext.output_format = OutputFormat::Plaintext;
        plaintext.quality.as_mut().unwrap().f1_score_layout = None;

        let aggregated = aggregate_new_format(&[plaintext]);

        assert!(
            aggregated.comparison.pareto_frontier.is_empty(),
            "a plaintext-only framework has no SF1 term and must never appear on the frontier"
        );
    }

    #[test]
    fn pages_per_sec_ranking_orders_by_median_descending() {
        let mut fast = create_test_result(
            "framework-fast",
            "pdf",
            OcrStatus::NotUsed,
            1_000,
            1_000_000.0,
            10_000_000,
        );
        fast.pdf_metadata = Some(pdf_metadata_with_page_count(100));

        let mut slow = create_test_result(
            "framework-slow",
            "pdf",
            OcrStatus::NotUsed,
            4_000,
            1_000_000.0,
            10_000_000,
        );
        slow.pdf_metadata = Some(pdf_metadata_with_page_count(100));

        let aggregated = aggregate_new_format(&[fast, slow]);
        let ranking = &aggregated.comparison.pages_per_sec_ranking;

        assert_eq!(ranking.len(), 2);
        assert!(ranking[0].framework_mode.contains("framework-fast"));
        assert_eq!(ranking[0].rank, 1);
        assert_eq!(ranking[0].value, 100.0);
        assert!(ranking[1].framework_mode.contains("framework-slow"));
        assert_eq!(ranking[1].rank, 2);
        assert_eq!(ranking[1].value, 25.0);
    }

    #[test]
    fn cpu_seconds_ranking_orders_ascending_lowest_first() {
        let mut lean = create_test_result(
            "framework-lean",
            "pdf",
            OcrStatus::NotUsed,
            1_000,
            1_000_000.0,
            10_000_000,
        );
        lean.metrics.cpu_seconds = 0.5;

        let mut heavy = create_test_result(
            "framework-heavy",
            "pdf",
            OcrStatus::NotUsed,
            1_000,
            1_000_000.0,
            10_000_000,
        );
        heavy.metrics.cpu_seconds = 4.0;

        let aggregated = aggregate_new_format(&[lean, heavy]);
        let ranking = &aggregated.comparison.cpu_seconds_ranking;

        assert_eq!(ranking.len(), 2);
        assert!(ranking[0].framework_mode.contains("framework-lean"));
        assert_eq!(ranking[0].rank, 1);
        assert_eq!(ranking[0].value, 0.5);
        assert!(ranking[1].framework_mode.contains("framework-heavy"));
        assert_eq!(ranking[1].rank, 2);
        assert_eq!(ranking[1].value, 4.0);
        assert!(
            ranking[1].relative > 1.0,
            "the higher-CPU framework's relative value should exceed the lowest-CPU baseline"
        );
    }
}
