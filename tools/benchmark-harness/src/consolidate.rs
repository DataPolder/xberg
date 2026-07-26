//! Loading benchmark results from disk for consolidation
//!
//! This module provides `load_run_results` which recursively loads benchmark
//! result JSON files from a directory tree, tagging them with batch mode info
//! inferred from directory names.

use crate::types::BenchmarkResult;
use crate::{Error, Result};
use std::fs;
use std::path::Path;

/// Load benchmark results from `results.json` files in a directory.
///
/// Recursively walks the given directory, loading any `results.json` files found.
/// For canonical batch directories (`batch`, `batch-*`, or legacy `*-batch`), the
/// framework name in each result is suffixed with `-batch` so that the aggregation
/// layer can distinguish single- vs batch-mode results.
///
/// # Errors
///
/// Returns [`Error::Io`] if the directory cannot be read, or [`Error::Benchmark`]
/// if a `results.json` file contains invalid JSON or fails validation.
pub fn load_run_results(dir: &Path) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();
    for entry in fs::read_dir(dir).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        let path = entry.path();

        if path.is_file() && path.file_name().is_some_and(|n| n == "results.json") {
            eprintln!("Loading results from {}", path.display());
            let json_content = fs::read_to_string(&path).map_err(Error::Io)?;
            let mut run_results: Vec<BenchmarkResult> = serde_json::from_str(&json_content)
                .map_err(|e| Error::Benchmark(format!("Failed to parse {}: {}", path.display(), e)))?;

            let is_batch = is_batch_results_dir(dir);

            if is_batch {
                for result in &mut run_results {
                    if !result.framework.ends_with("-batch") {
                        result.framework = format!("{}-batch", result.framework);
                    }
                }
            }

            for result in &run_results {
                crate::output::validate_result(result)
                    .map_err(|e| Error::Benchmark(format!("Invalid result in {}: {}", path.display(), e)))?;
            }

            results.extend(run_results);
        } else if path.is_dir() {
            let mut run_results = load_run_results(&path)?;
            results.append(&mut run_results);
        }
    }
    Ok(results)
}

fn is_batch_results_dir(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    name == "batch" || name.starts_with("batch-") || name.ends_with("-batch")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate_new_format;
    use crate::types::{ErrorKind, FrameworkCapabilities, OutputFormat, PerformanceMetrics};
    use std::time::Duration;

    /// Build a minimal valid `BenchmarkResult` for testing.
    fn make_result(framework: &str) -> BenchmarkResult {
        BenchmarkResult {
            framework: framework.to_string(),
            file_path: std::path::PathBuf::from("test.pdf"),
            file_size: 1024,
            success: true,
            error_message: None,
            error_kind: ErrorKind::None,
            duration: Duration::from_millis(100),
            extraction_duration: None,
            subprocess_overhead: None,
            metrics: PerformanceMetrics {
                baseline_memory_bytes: 0,
                peak_memory_bytes: 1_000_000,
                peak_memory_delta_bytes: 1_000_000,
                avg_cpu_percent: 50.0,
                cpu_seconds: 50.0,
                throughput_bytes_per_sec: 10_240.0,
                p50_memory_bytes: 900_000,
                p95_memory_bytes: 950_000,
                p99_memory_bytes: 990_000,
            },
            quality: None,
            iterations: vec![],
            statistics: None,
            cold_start_duration: None,
            file_extension: "pdf".to_string(),
            framework_capabilities: FrameworkCapabilities::default(),
            pdf_metadata: None,
            ocr_status: Default::default(),
            extracted_text: None,
            system_load: None,
            output_format: OutputFormat::Markdown,
        }
    }

    #[test]
    fn test_load_single_results_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let results = vec![make_result("xberg-rust")];
        let json = serde_json::to_string(&results).expect("serialize");
        fs::write(dir.path().join("results.json"), &json).expect("write");

        let loaded = load_run_results(dir.path()).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].framework, "xberg-rust");
    }

    #[test]
    fn test_batch_directory_tags_framework_name() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let batch_dir = dir.path().join("xberg-rust-batch");
        fs::create_dir_all(&batch_dir).expect("create subdir");

        let results = vec![make_result("xberg-rust")];
        let json = serde_json::to_string(&results).expect("serialize");
        fs::write(batch_dir.join("results.json"), &json).expect("write");

        let loaded = load_run_results(dir.path()).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].framework, "xberg-rust-batch");
    }

    #[test]
    fn test_batch_suffix_not_doubled() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let batch_dir = dir.path().join("xberg-rust-batch");
        fs::create_dir_all(&batch_dir).expect("create subdir");

        let results = vec![make_result("xberg-rust-batch")];
        let json = serde_json::to_string(&results).expect("serialize");
        fs::write(batch_dir.join("results.json"), &json).expect("write");

        let loaded = load_run_results(dir.path()).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].framework, "xberg-rust-batch");
    }

    #[test]
    fn canonical_batch_heuristic_tags_liteparse_and_aggregates_as_batch() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let batch_dir = dir.path().join("batch-heuristic");
        fs::create_dir_all(&batch_dir).expect("create subdir");

        let results = vec![make_result("liteparse")];
        fs::write(
            batch_dir.join("results.json"),
            serde_json::to_string(&results).expect("serialize"),
        )
        .expect("write");

        let loaded = load_run_results(dir.path()).expect("load");
        assert_eq!(loaded[0].framework, "liteparse-batch");

        let aggregated = aggregate_new_format(&loaded);
        assert!(aggregated.by_framework_mode.contains_key("liteparse:markdown:batch"));
        assert!(!aggregated.by_framework_mode.contains_key("liteparse:markdown:single"));
    }

    #[test]
    fn canonical_batch_ocr_tags_xberg_without_doubling_suffix() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let batch_dir = dir.path().join("batch-ocr");
        fs::create_dir_all(&batch_dir).expect("create subdir");

        let results = vec![make_result("xberg-rust-batch")];
        fs::write(
            batch_dir.join("results.json"),
            serde_json::to_string(&results).expect("serialize"),
        )
        .expect("write");

        let loaded = load_run_results(dir.path()).expect("load");
        assert_eq!(loaded[0].framework, "xberg-rust-batch");
    }

    #[test]
    fn test_recursive_loading() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let sub1 = dir.path().join("framework-a");
        let sub2 = dir.path().join("framework-b");
        fs::create_dir_all(&sub1).expect("create subdir 1");
        fs::create_dir_all(&sub2).expect("create subdir 2");

        fs::write(
            sub1.join("results.json"),
            serde_json::to_string(&vec![make_result("framework-a")]).expect("serialize"),
        )
        .expect("write a");
        fs::write(
            sub2.join("results.json"),
            serde_json::to_string(&vec![make_result("framework-b")]).expect("serialize"),
        )
        .expect("write b");

        let loaded = load_run_results(dir.path()).expect("load");
        assert_eq!(loaded.len(), 2);
        let names: Vec<&str> = loaded.iter().map(|r| r.framework.as_str()).collect();
        assert!(names.contains(&"framework-a"));
        assert!(names.contains(&"framework-b"));
    }

    #[test]
    fn test_malformed_json_returns_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(dir.path().join("results.json"), "NOT VALID JSON").expect("write");

        let result = load_run_results(dir.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Failed to parse"));
    }

    #[test]
    fn malformed_nested_results_propagate_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let nested = dir.path().join("nested").join("deeper");
        fs::create_dir_all(&nested).expect("create nested dirs");
        fs::write(nested.join("results.json"), "NOT VALID JSON").expect("write");

        let error = load_run_results(dir.path()).unwrap_err();
        assert!(error.to_string().contains("Failed to parse"));
        assert!(error.to_string().contains("nested/deeper/results.json"));
    }

    #[test]
    fn invalid_nested_result_propagates_validation_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let nested = dir.path().join("nested");
        fs::create_dir_all(&nested).expect("create nested dir");
        let mut invalid = make_result("liteparse");
        invalid.error_message = Some("inconsistent success state".to_string());
        fs::write(
            nested.join("results.json"),
            serde_json::to_string(&vec![invalid]).expect("serialize"),
        )
        .expect("write");

        let error = load_run_results(dir.path()).unwrap_err();
        assert!(error.to_string().contains("Invalid result"));
        assert!(error.to_string().contains("nested/results.json"));
    }

    #[test]
    fn test_empty_directory_returns_empty_vec() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let loaded = load_run_results(dir.path()).expect("load");
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_nonexistent_directory_returns_error() {
        let result = load_run_results(Path::new("/tmp/nonexistent_benchmark_dir_12345"));
        assert!(result.is_err());
    }
}
