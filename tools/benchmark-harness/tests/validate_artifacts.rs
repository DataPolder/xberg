//! Integration tests for the benchmark artifact/aggregate release-contract validator.
//!
//! Ported from `tools/benchmark-harness/tests/test_validate_benchmark_artifacts.py`, which
//! exercised `scripts/ci/benchmarks/validate-benchmark-artifacts.py`. Real cohort manifests and
//! fixture descriptors are copied from the repository (so a pinned-digest drift is caught), but
//! referenced documents are synthetic — the validator never reads document content, only its
//! BLAKE3 digest and byte length, which the synthetic bytes provide consistently.

use benchmark_harness::aggregate::{
    ComparisonData, ConsolidationMetadata, FileTypeAggregation, FrameworkModeAggregation, NewConsolidatedResults,
    PerFixtureRow, Percentiles, PerformancePercentiles, SCHEMA_VERSION,
};
use benchmark_harness::bench_matrix::{Cohort, CohortContract};
use benchmark_harness::provenance::{
    CorpusProvenance, FixtureProvenance, FrameworkProvenance, RepositoryProvenance, RunProvenance, TimingProvenance,
};
use benchmark_harness::types::{
    BenchmarkResult, ErrorKind, FrameworkCapabilities, IterationResult, OcrStatus, PerformanceMetrics,
};
use benchmark_harness::validate_artifacts::{ValidateArtifactsArgs, validate};
use benchmark_harness::{Error, write_json, write_run_provenance};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

const SOURCE_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RUN_ID: &str = "42";
const ITERATIONS: usize = 3;

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn blake3_hex(path: &Path) -> String {
    blake3::hash(&std::fs::read(path).expect("read file to hash"))
        .to_hex()
        .to_string()
}

fn zero_metrics() -> PerformanceMetrics {
    PerformanceMetrics {
        baseline_memory_bytes: 0,
        peak_memory_bytes: 0,
        peak_memory_delta_bytes: 0,
        avg_cpu_percent: 0.0,
        cpu_seconds: 0.0,
        throughput_bytes_per_sec: 0.0,
        p50_memory_bytes: 0,
        p95_memory_bytes: 0,
        p99_memory_bytes: 0,
    }
}

/// A cohort's fixture tree, materialized under a temp root exactly like the release CI layout:
/// real cohort manifest + fixture descriptor bytes (so pinned digests stay honest), synthetic
/// document bytes (the validator never reads document content).
struct FixtureTree {
    _root: TempDir,
    cohort_manifest: PathBuf,
    fixtures_root: PathBuf,
    manifest_blake3: String,
    fixture_provenance: Vec<FixtureProvenance>,
}

fn materialize_fixture_tree(cohort: Cohort, contract: &CohortContract) -> FixtureTree {
    let root = tempfile::tempdir().expect("tempdir");
    let cohort_manifest = root.path().join("cohort.json");
    let fixtures_root = root.path().join("tools/benchmark-harness/fixtures");
    std::fs::create_dir_all(&fixtures_root).expect("create fixtures root");

    let cohort_slug = match cohort {
        Cohort::Native => "native-pdf-fast-b8",
        Cohort::Ocr => "ocr-pdf-fast-b4",
        Cohort::Office => "native-office-fast",
        Cohort::Markup => "native-markup-fast",
        Cohort::Ebook => "native-ebook-fast",
        Cohort::Email => "native-email-fast",
        Cohort::Data => "native-data-fast",
        Cohort::Images => "ocr-images-fast",
    };
    std::fs::copy(repo_path(&format!("cohorts/{cohort_slug}.json")), &cohort_manifest).expect("copy cohort manifest");
    let manifest_blake3 = blake3_hex(&cohort_manifest);

    let mut fixture_provenance = Vec::with_capacity(contract.fixtures.len());
    for (fixture, document_stem) in contract.fixtures.iter().zip(contract.document_stems.iter()) {
        let descriptor_path = fixtures_root.join(fixture);
        std::fs::create_dir_all(descriptor_path.parent().expect("descriptor has a parent"))
            .expect("create descriptor dir");
        std::fs::copy(repo_path(&format!("fixtures/{fixture}")), &descriptor_path).expect("copy fixture descriptor");

        let descriptor: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&descriptor_path).expect("read descriptor"))
                .expect("parse descriptor");
        let document_field = descriptor["document"]
            .as_str()
            .expect("descriptor has a document field");
        let document_path = descriptor_path
            .parent()
            .expect("descriptor has a parent")
            .join(document_field);
        std::fs::create_dir_all(document_path.parent().expect("document has a parent")).expect("create document dir");
        std::fs::write(
            &document_path,
            format!("temporary benchmark document: {document_stem}\n"),
        )
        .expect("write document");

        fixture_provenance.push(FixtureProvenance {
            fixture: (*fixture).to_string(),
            fixture_blake3: blake3_hex(&descriptor_path),
            document_blake3: blake3_hex(&document_path),
            document_bytes: std::fs::metadata(&document_path).expect("stat document").len(),
        });
    }

    FixtureTree {
        _root: root,
        cohort_manifest,
        fixtures_root,
        manifest_blake3,
        fixture_provenance,
    }
}

/// The framework name the runner actually writes into artifacts. Xberg batch cells carry a
/// `-batch` suffix (see `adapters::xberg` and `normalize_run_frameworks`); competitors and
/// single-file cells use the bare name. Mirroring this here keeps the fixtures faithful to real
/// artifacts so the validator's suffix handling is exercised.
fn runtime_framework_name(entry: &benchmark_harness::bench_matrix::MatrixEntry) -> String {
    use benchmark_harness::bench_matrix::ExecutionMode;

    if matches!(entry.mode, ExecutionMode::Batch) && entry.framework.starts_with("xberg-") {
        format!("{}-batch", entry.framework)
    } else {
        entry.framework.clone()
    }
}

fn build_provenance(
    entry: &benchmark_harness::bench_matrix::MatrixEntry,
    contract: &CohortContract,
    manifest_blake3: &str,
    fixture_provenance: &[FixtureProvenance],
) -> RunProvenance {
    use benchmark_harness::bench_matrix::ExecutionMode;

    let batch = matches!(entry.mode, ExecutionMode::Batch);
    RunProvenance {
        schema_version: 2,
        harness_version: "test".to_string(),
        repository: RepositoryProvenance {
            commit: Some(SOURCE_SHA.to_string()),
            dirty: Some(false),
        },
        corpus: CorpusProvenance {
            cohort: Some(contract.manifest_name.to_string()),
            cohort_manifest_blake3: Some(manifest_blake3.to_string()),
            ordered_fixtures: fixture_provenance.to_vec(),
        },
        frameworks: vec![FrameworkProvenance {
            name: runtime_framework_name(entry),
            version: "0.0.0".to_string(),
            executable: None,
            models: Vec::new(),
            batch_capability: None,
            requested_workers: None,
            effective_workers: None,
            configured_thread_budget: None,
            worker_semantics: "test".to_string(),
            effective_warmup_iterations: 0,
            eligible_documents: contract.fixtures.len(),
            batch_partitions: batch.then(|| contract.fixtures.len() / contract.batch_size),
        }],
        timing: TimingProvenance {
            mode: entry.mode.benchmark_mode(),
            warmup_iterations: 0,
            benchmark_iterations: ITERATIONS,
            timeout_ms: 0,
            output_format: entry.output_format,
        },
        fixed_batch_size: batch.then_some(contract.batch_size),
    }
}

fn build_results(
    entry: &benchmark_harness::bench_matrix::MatrixEntry,
    contract: &CohortContract,
    cohort: Cohort,
) -> Vec<BenchmarkResult> {
    contract
        .document_stems
        .iter()
        .map(|stem| BenchmarkResult {
            framework: runtime_framework_name(entry),
            output_format: entry.output_format,
            file_path: PathBuf::from(format!("/workspace/test_documents/{stem}.pdf")),
            file_size: 1,
            success: true,
            error_message: None,
            error_kind: ErrorKind::None,
            duration: Duration::from_millis(1),
            extraction_duration: None,
            subprocess_overhead: None,
            metrics: zero_metrics(),
            quality: None,
            iterations: (0..ITERATIONS)
                .map(|index| IterationResult {
                    // mirror that here so the fixture matches real artifacts.
                    iteration: index + 1,
                    duration: Duration::from_millis(1),
                    extraction_duration: None,
                    metrics: zero_metrics(),
                })
                .collect(),
            statistics: None,
            cold_start_duration: None,
            file_extension: "pdf".to_string(),
            framework_capabilities: FrameworkCapabilities::default(),
            pdf_metadata: None,
            ocr_status: if cohort.expects_ocr() {
                OcrStatus::Used
            } else {
                OcrStatus::NotUsed
            },
            extracted_text: None,
            system_load: None,
        })
        .collect()
}

/// A fully materialized, contract-conformant artifact tree ready to validate.
struct ArtifactScenario {
    _tree: FixtureTree,
    args: ValidateArtifactsArgs,
    contract: CohortContract,
}

fn artifact_scenario(cohort: Cohort) -> ArtifactScenario {
    let contract = cohort.contract();
    let tree = materialize_fixture_tree(cohort, &contract);
    let artifacts_dir = tree.cohort_manifest.parent().expect("root").join("artifacts");
    std::fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");

    for entry in &contract.matrix {
        let run_dir = artifacts_dir.join(format!("{}-{RUN_ID}", entry.artifact)).join("run");
        let provenance = build_provenance(entry, &contract, &tree.manifest_blake3, &tree.fixture_provenance);
        write_run_provenance(&provenance, &run_dir.join("provenance.json")).expect("write provenance");
        let results = build_results(entry, &contract, cohort);
        write_json(&results, &run_dir.join("results.json")).expect("write results");
    }

    let args = ValidateArtifactsArgs {
        cohort,
        aggregated_file: None,
        artifacts_dir: Some(artifacts_dir),
        cohort_manifest: Some(tree.cohort_manifest.clone()),
        fixtures_root: Some(tree.fixtures_root.clone()),
        source_sha: Some(SOURCE_SHA.to_string()),
        run_id: Some(RUN_ID.to_string()),
        iterations: ITERATIONS,
    };

    ArtifactScenario {
        _tree: tree,
        args,
        contract,
    }
}

fn provenance_path(scenario: &ArtifactScenario, matrix_index: usize) -> PathBuf {
    let entry = &scenario.contract.matrix[matrix_index];
    scenario
        .args
        .artifacts_dir
        .as_ref()
        .unwrap()
        .join(format!("{}-{RUN_ID}", entry.artifact))
        .join("run/provenance.json")
}

fn results_path(scenario: &ArtifactScenario, matrix_index: usize) -> PathBuf {
    let entry = &scenario.contract.matrix[matrix_index];
    scenario
        .args
        .artifacts_dir
        .as_ref()
        .unwrap()
        .join(format!("{}-{RUN_ID}", entry.artifact))
        .join("run/results.json")
}

fn tamper_json(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    mutate(&mut value);
    std::fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

fn assert_err_contains<T: std::fmt::Debug>(result: Result<T, Error>, needle: &str) {
    let error = result.expect_err("expected validation failure");
    let message = error.to_string();
    assert!(
        message.contains(needle),
        "expected error containing {needle:?}, got: {message}"
    );
}

fn required_count(contract: &CohortContract) -> usize {
    contract.matrix.iter().filter(|entry| !entry.optional).count()
}

fn optional_artifact_dir(scenario: &ArtifactScenario) -> PathBuf {
    let entry = scenario
        .contract
        .matrix
        .iter()
        .find(|entry| entry.optional)
        .expect("cohort has an optional (best-effort) entry");
    scenario
        .args
        .artifacts_dir
        .as_ref()
        .unwrap()
        .join(format!("{}-{RUN_ID}", entry.artifact))
}

#[test]
fn accepts_exact_native_contract() {
    let scenario = artifact_scenario(Cohort::Native);
    let required = required_count(&scenario.contract);
    let message = validate(&scenario.args).expect("native contract should validate");
    assert_eq!(message, format!("validated {required} native benchmark artifacts"));
}

#[test]
fn accepts_exact_ocr_contract() {
    let scenario = artifact_scenario(Cohort::Ocr);
    let required = required_count(&scenario.contract);
    let message = validate(&scenario.args).expect("ocr contract should validate");
    assert_eq!(message, format!("validated {required} ocr benchmark artifacts"));
}

/// Index of a batch-mode xberg cell in the native matrix — the case that carries the runner's
/// `-batch` framework-name suffix.
fn batch_xberg_index(contract: &CohortContract) -> usize {
    use benchmark_harness::bench_matrix::ExecutionMode;
    contract
        .matrix
        .iter()
        .position(|entry| entry.framework.starts_with("xberg-") && matches!(entry.mode, ExecutionMode::Batch))
        .expect("native matrix has a batch xberg cell")
}

#[test]
fn accepts_batch_xberg_framework_with_mode_suffix() {
    // Batch xberg cells write `xberg-<fmt>-<pipeline>-batch`; the validator must accept that
    // suffixed name against the mode-independent matrix entry (mode is checked via timing.mode).
    let scenario = artifact_scenario(Cohort::Native);
    let index = batch_xberg_index(&scenario.contract);
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(provenance_path(&scenario, index)).unwrap()).unwrap();
    assert!(
        value["frameworks"][0]["name"].as_str().unwrap().ends_with("-batch"),
        "fixture must carry the runner's -batch suffix"
    );
    validate(&scenario.args).expect("suffixed batch xberg name must validate");
}

#[test]
fn rejects_batch_xberg_framework_with_wrong_base() {
    // Stripping the mode suffix must not mask a genuinely wrong base framework name.
    let scenario = artifact_scenario(Cohort::Native);
    let index = batch_xberg_index(&scenario.contract);
    tamper_json(&provenance_path(&scenario, index), |value| {
        value["frameworks"][0]["name"] = serde_json::json!("xberg-markdown-bogus-batch");
    });
    assert_err_contains(validate(&scenario.args), "framework mismatch");
}

#[test]
fn accepts_native_contract_when_optional_mineru_absent() {
    // A best-effort framework (MinerU) that never produced an artifact must not fail
    // validation: the baseline still publishes with the required frameworks.
    let scenario = artifact_scenario(Cohort::Native);
    std::fs::remove_dir_all(optional_artifact_dir(&scenario)).expect("remove optional artifact dir");
    let required = required_count(&scenario.contract);
    let message = validate(&scenario.args).expect("native contract should validate without mineru");
    assert_eq!(message, format!("validated {required} native benchmark artifacts"));
}

#[test]
fn accepts_native_contract_when_optional_mineru_failed() {
    // A present-but-failed best-effort artifact is ignored, not rejected.
    let scenario = artifact_scenario(Cohort::Native);
    let results = optional_artifact_dir(&scenario).join("run/results.json");
    tamper_json(&results, |value| {
        value[0]["success"] = serde_json::Value::Bool(false);
        value[0]["error_kind"] = serde_json::Value::String("timeout".to_string());
    });
    validate(&scenario.args).expect("a failed optional framework must not fail validation");
}

#[test]
fn rejects_tampered_manifest_bytes() {
    let scenario = artifact_scenario(Cohort::Native);
    let mut manifest = std::fs::read(scenario.args.cohort_manifest.clone().unwrap()).unwrap();
    manifest.push(b' ');
    std::fs::write(scenario.args.cohort_manifest.as_ref().unwrap(), manifest).unwrap();
    assert_err_contains(validate(&scenario.args), "manifest BLAKE3 mismatch");
}

#[test]
fn rejects_wrong_fixture_digest_for_real_descriptor() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&provenance_path(&scenario, 0), |value| {
        value["corpus"]["ordered_fixtures"][0]["fixture_blake3"] = serde_json::Value::String("0".repeat(64));
    });
    assert_err_contains(validate(&scenario.args), "descriptor BLAKE3 mismatch");
}

#[test]
fn rejects_wrong_document_digest_for_real_document() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&provenance_path(&scenario, 0), |value| {
        value["corpus"]["ordered_fixtures"][0]["document_blake3"] = serde_json::Value::String("0".repeat(64));
    });
    assert_err_contains(validate(&scenario.args), "document BLAKE3 mismatch");
}

#[test]
fn rejects_wrong_document_bytes_for_real_document() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&provenance_path(&scenario, 0), |value| {
        let bytes = value["corpus"]["ordered_fixtures"][0]["document_bytes"]
            .as_u64()
            .unwrap();
        value["corpus"]["ordered_fixtures"][0]["document_bytes"] = serde_json::Value::from(bytes + 1);
    });
    assert_err_contains(validate(&scenario.args), "document size mismatch");
}

#[test]
fn rejects_unexpected_artifact() {
    let scenario = artifact_scenario(Cohort::Native);
    std::fs::create_dir(
        scenario
            .args
            .artifacts_dir
            .as_ref()
            .unwrap()
            .join("benchmarks-surprise-42"),
    )
    .unwrap();
    assert_err_contains(validate(&scenario.args), "unexpected");
}

#[test]
fn rejects_source_sha_mismatch() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&provenance_path(&scenario, 0), |value| {
        value["repository"]["commit"] = serde_json::Value::String("d".repeat(40));
    });
    assert_err_contains(validate(&scenario.args), "source SHA mismatch");
}

#[test]
fn rejects_timeout_result() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&results_path(&scenario, 0), |value| {
        value[0]["success"] = serde_json::Value::Bool(false);
        value[0]["error_kind"] = serde_json::Value::String("timeout".to_string());
        value[0]["error_message"] = serde_json::Value::String("timed out".to_string());
    });
    assert_err_contains(validate(&scenario.args), "failed");
}

#[test]
fn rejects_duplicate_fixture_result() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&results_path(&scenario, 0), |value| {
        let first_path = value[0]["file_path"].clone();
        value[1]["file_path"] = first_path;
    });
    assert_err_contains(validate(&scenario.args), "order/content mismatch");
}

#[test]
fn rejects_malformed_provenance() {
    let scenario = artifact_scenario(Cohort::Native);
    std::fs::write(provenance_path(&scenario, 0), "{").unwrap();
    assert_err_contains(validate(&scenario.args), "malformed");
}

#[test]
fn rejects_manifest_fixtures_when_not_an_array() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(scenario.args.cohort_manifest.as_ref().unwrap(), |value| {
        value["fixtures"] = serde_json::json!({});
    });
    assert!(validate(&scenario.args).is_err());
}

#[test]
fn rejects_framework_when_not_an_object() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&provenance_path(&scenario, 0), |value| {
        value["frameworks"] = serde_json::json!([null]);
    });
    assert!(validate(&scenario.args).is_err());
}

#[test]
fn rejects_result_row_when_not_an_object() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&results_path(&scenario, 0), |value| {
        value[0] = serde_json::Value::Null;
    });
    assert!(validate(&scenario.args).is_err());
}

#[test]
fn accepts_sequential_one_based_iterations() {
    // The runner numbers iterations 1-based ([1, 2, 3] for ITERATIONS = 3); the untampered
    // scenario fixture already reflects that, so this simply pins the happy path against
    // regressing back to a 0-based expectation.
    let scenario = artifact_scenario(Cohort::Native);
    validate(&scenario.args).expect("sequential 1-based iterations should validate");
}

#[test]
fn rejects_misordered_iterations() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&results_path(&scenario, 0), |value| {
        let second = value[0]["iterations"][1]["iteration"].clone();
        let third = value[0]["iterations"][2]["iteration"].clone();
        value[0]["iterations"][1]["iteration"] = third;
        value[0]["iterations"][2]["iteration"] = second;
    });
    assert_err_contains(validate(&scenario.args), "iteration order/duplicates mismatch");
}

#[test]
fn rejects_duplicate_iterations() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&results_path(&scenario, 0), |value| {
        let first = value[0]["iterations"][0]["iteration"].clone();
        value[0]["iterations"][1]["iteration"] = first;
    });
    assert_err_contains(validate(&scenario.args), "iteration order/duplicates mismatch");
}

#[test]
fn rejects_iteration_count_mismatch() {
    let scenario = artifact_scenario(Cohort::Native);
    tamper_json(&results_path(&scenario, 0), |value| {
        // the configured ITERATIONS.
        value[0]["iterations"].as_array_mut().unwrap().pop();
    });
    assert_err_contains(validate(&scenario.args), "iteration count mismatch");
}

fn zero_percentiles() -> Percentiles {
    Percentiles {
        p50: 0.0,
        p95: 0.0,
        p99: 0.0,
    }
}

fn zero_bucket(total_sample_count: usize) -> PerformancePercentiles {
    PerformancePercentiles {
        successful_sample_count: total_sample_count,
        performance_sample_count: total_sample_count,
        total_sample_count,
        framework_errors: 0,
        harness_errors: 0,
        config_setup_errors: 0,
        timeouts: 0,
        empty_content: 0,
        error_details: HashMap::new(),
        throughput: zero_percentiles(),
        memory: zero_percentiles(),
        duration: zero_percentiles(),
        success_rate_percent: 100.0,
        extraction_duration: None,
        quality: None,
        pages_per_sec: None,
        cpu_seconds: Percentiles::default(),
        batch_size: None,
        system_load: None,
        throughput_excluded_sample_count: 0,
    }
}

fn build_aggregate(contract: &CohortContract, cohort: Cohort) -> NewConsolidatedResults {
    // Real per-extension buckets derived from the contract, so family cohorts (office, images, …)
    // produce the multi-file-type shape the validator now checks — not a single fake "pdf" bucket.
    let mut ext_counts: HashMap<&str, usize> = HashMap::new();
    for extension in contract.document_extensions {
        *ext_counts.entry(*extension).or_insert(0) += 1;
    }
    let mut by_framework_mode = HashMap::new();
    for entry in &contract.matrix {
        let mut by_file_type = HashMap::new();
        for (extension, count) in &ext_counts {
            let bucket = zero_bucket(*count);
            by_file_type.insert(
                (*extension).to_string(),
                FileTypeAggregation {
                    file_type: (*extension).to_string(),
                    no_ocr: if cohort.expects_ocr() {
                        None
                    } else {
                        Some(bucket.clone())
                    },
                    with_ocr: if cohort.expects_ocr() { Some(bucket) } else { None },
                },
            );
        }
        by_framework_mode.insert(
            entry.aggregate_key(),
            FrameworkModeAggregation {
                framework: entry.framework.clone(),
                output_format: entry.output_format,
                mode: entry.mode.aggregate_slug().to_string(),
                cold_start: None,
                overall_performance: None,
                by_file_type,
            },
        );
    }

    let per_fixture_results = contract
        .matrix
        .iter()
        .flat_map(|entry| {
            contract
                .document_stems
                .iter()
                .zip(contract.document_extensions.iter())
                .map(move |(stem, extension)| PerFixtureRow {
                    framework: entry.framework.clone(),
                    output_format: entry.output_format,
                    execution_mode: entry.mode.aggregate_slug().to_string(),
                    ocr: Some(cohort.expects_ocr()),
                    fixture_id: (*stem).to_string(),
                    file_type: (*extension).to_string(),
                    duration_ms: 0.0,
                    peak_memory_mb: 0.0,
                    f1_text: None,
                    f1_layout: None,
                    f1_numeric: None,
                    quality_score: None,
                    correct: None,
                    success: true,
                    error_kind: None,
                    file_size: 1,
                    throughput_bytes_per_sec: 0.0,
                    avg_cpu_percent: 0.0,
                    cpu_seconds: 0.0,
                    baseline_memory_bytes: 0,
                    peak_memory_delta_bytes: 0,
                    p50_memory_bytes: 0,
                    p95_memory_bytes: 0,
                    p99_memory_bytes: 0,
                    extraction_duration_ms: None,
                    subprocess_overhead_ms: None,
                    cold_start_duration_ms: None,
                    error_message: None,
                    quality: None,
                    pdf_metadata: None,
                    framework_capabilities: FrameworkCapabilities::default(),
                    system_load: None,
                    iterations: Vec::new(),
                    statistics: None,
                })
        })
        .collect();

    NewConsolidatedResults {
        schema_version: SCHEMA_VERSION.to_string(),
        by_framework_mode,
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
            pareto_frontier: Vec::new(),
            deltas_vs_baseline: HashMap::new(),
        },
        per_fixture_results,
        metadata: ConsolidationMetadata {
            total_results: contract.matrix.len() * contract.fixtures.len(),
            framework_count: contract.matrix.len(),
            file_type_count: 1,
            shared_corpus_markdown: Vec::new(),
            shared_corpus_plaintext: Vec::new(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            disk_size_conflicts: Vec::new(),
        },
        run_provenance: Vec::new(),
    }
}

fn write_aggregate(aggregate: &NewConsolidatedResults) -> (TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("aggregated.json");
    std::fs::write(&path, serde_json::to_string_pretty(aggregate).unwrap()).unwrap();
    (root, path)
}

fn aggregate_args(cohort: Cohort, aggregated_file: PathBuf) -> ValidateArtifactsArgs {
    ValidateArtifactsArgs {
        cohort,
        aggregated_file: Some(aggregated_file),
        artifacts_dir: None,
        cohort_manifest: None,
        fixtures_root: None,
        source_sha: None,
        run_id: None,
        iterations: ITERATIONS,
    }
}

#[test]
fn accepts_exact_native_aggregate_contract() {
    let contract = Cohort::Native.contract();
    let aggregate = build_aggregate(&contract, Cohort::Native);
    let (_root, path) = write_aggregate(&aggregate);
    let required = required_count(&contract);
    let message = validate(&aggregate_args(Cohort::Native, path)).expect("native aggregate should validate");
    assert_eq!(
        message,
        format!(
            "validated {required} native aggregate keys and {} fixture rows",
            required * contract.fixtures.len()
        )
    );
}

#[test]
fn accepts_exact_ocr_aggregate_contract() {
    let contract = Cohort::Ocr.contract();
    let aggregate = build_aggregate(&contract, Cohort::Ocr);
    let (_root, path) = write_aggregate(&aggregate);
    let required = required_count(&contract);
    let message = validate(&aggregate_args(Cohort::Ocr, path)).expect("ocr aggregate should validate");
    assert_eq!(
        message,
        format!(
            "validated {required} ocr aggregate keys and {} fixture rows",
            required * contract.fixtures.len()
        )
    );
}

#[test]
fn accepts_native_aggregate_when_optional_mineru_absent() {
    // validation must still pass on the required frameworks.
    let contract = Cohort::Native.contract();
    let optional_entry = contract
        .matrix
        .iter()
        .find(|entry| entry.optional)
        .expect("native cohort has an optional entry");
    let optional_key = optional_entry.aggregate_key();
    let optional_framework = optional_entry.framework.clone();

    let mut aggregate = build_aggregate(&contract, Cohort::Native);
    aggregate.by_framework_mode.remove(&optional_key);
    aggregate
        .per_fixture_results
        .retain(|row| row.framework != optional_framework);

    let (_root, path) = write_aggregate(&aggregate);
    let required = required_count(&contract);
    let message =
        validate(&aggregate_args(Cohort::Native, path)).expect("native aggregate should validate without mineru");
    assert_eq!(
        message,
        format!(
            "validated {required} native aggregate keys and {} fixture rows",
            required * contract.fixtures.len()
        )
    );
}

#[test]
fn rejects_unexpected_aggregate_key() {
    let contract = Cohort::Native.contract();
    let aggregate = build_aggregate(&contract, Cohort::Native);
    let mut value = serde_json::to_value(&aggregate).unwrap();
    let map = value["by_framework_mode"].as_object_mut().unwrap();
    let (first_key, first_value) = map
        .iter()
        .next()
        .map(|(key, value)| (key.clone(), value.clone()))
        .unwrap();
    map.remove(&first_key);
    map.insert("surprise:markdown:single".to_string(), first_value);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("aggregated.json");
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    assert_err_contains(validate(&aggregate_args(Cohort::Native, path)), "unexpected");
}

#[test]
fn rejects_native_aggregate_with_only_with_ocr_bucket() {
    let contract = Cohort::Native.contract();
    let mut aggregate = build_aggregate(&contract, Cohort::Native);
    for group in aggregate.by_framework_mode.values_mut() {
        let file_group = group.by_file_type.get_mut("pdf").unwrap();
        file_group.with_ocr = file_group.no_ocr.take();
    }
    let (_root, path) = write_aggregate(&aggregate);
    assert!(validate(&aggregate_args(Cohort::Native, path)).is_err());
}

#[test]
fn rejects_ocr_aggregate_with_only_no_ocr_bucket() {
    let contract = Cohort::Ocr.contract();
    let mut aggregate = build_aggregate(&contract, Cohort::Ocr);
    for group in aggregate.by_framework_mode.values_mut() {
        let file_group = group.by_file_type.get_mut("pdf").unwrap();
        file_group.no_ocr = file_group.with_ocr.take();
    }
    let (_root, path) = write_aggregate(&aggregate);
    assert!(validate(&aggregate_args(Cohort::Ocr, path)).is_err());
}

#[test]
fn rejects_aggregate_group_without_file_type_metrics() {
    // A group with no file-type buckets can never account for the cohort's fixtures.
    let contract = Cohort::Native.contract();
    let mut aggregate = build_aggregate(&contract, Cohort::Native);
    let first_group = aggregate.by_framework_mode.values_mut().next().unwrap();
    first_group.by_file_type.clear();
    let (_root, path) = write_aggregate(&aggregate);
    assert_err_contains(
        validate(&aggregate_args(Cohort::Native, path)),
        "has no file-type metrics",
    );
}

#[test]
fn accepts_aggregate_with_multiple_file_type_buckets() {
    // The office cohort spans 7 extensions (docx×2, doc, pptx, ppt, xlsx, odt, rtf); build_aggregate
    // now emits that real per-extension bucket shape, which must validate.
    let contract = Cohort::Office.contract();
    let aggregate = build_aggregate(&contract, Cohort::Office);
    let (_root, path) = write_aggregate(&aggregate);
    validate(&aggregate_args(Cohort::Office, path)).expect("multi-file-type office aggregate should validate");
}

#[test]
fn rejects_aggregate_with_unexpected_file_type_bucket() {
    // A file-type bucket the cohort's fixtures don't contain must be rejected, even if counts sum.
    let contract = Cohort::Office.contract();
    let mut aggregate = build_aggregate(&contract, Cohort::Office);
    let group = aggregate.by_framework_mode.values_mut().next().unwrap();
    // Move all `docx` samples (2) into a bogus `pdf` bucket: totals still sum to 8, but pdf is not
    // an office extension and docx is now missing.
    let docx = group.by_file_type.remove("docx").unwrap();
    group.by_file_type.insert(
        "pdf".to_string(),
        FileTypeAggregation {
            file_type: "pdf".to_string(),
            ..docx
        },
    );
    let (_root, path) = write_aggregate(&aggregate);
    assert_err_contains(validate(&aggregate_args(Cohort::Office, path)), "file-type buckets");
}

#[test]
fn rejects_aggregate_with_wrong_file_type_sample_count() {
    // The right extensions but a mis-bucketed count (docx should be 2, not 1) must be rejected —
    // the per-extension check the sum-only check used to miss.
    let contract = Cohort::Office.contract();
    let mut aggregate = build_aggregate(&contract, Cohort::Office);
    let group = aggregate.by_framework_mode.values_mut().next().unwrap();
    group
        .by_file_type
        .get_mut("docx")
        .unwrap()
        .no_ocr
        .as_mut()
        .unwrap()
        .total_sample_count = 1;
    let (_root, path) = write_aggregate(&aggregate);
    assert_err_contains(
        validate(&aggregate_args(Cohort::Office, path)),
        "file type docx covers 1 samples, expected 2",
    );
}

#[test]
fn rejects_aggregate_row_when_not_an_object() {
    let contract = Cohort::Native.contract();
    let aggregate = build_aggregate(&contract, Cohort::Native);
    let mut value = serde_json::to_value(&aggregate).unwrap();
    value["per_fixture_results"][0] = serde_json::Value::Null;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("aggregated.json");
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    assert!(validate(&aggregate_args(Cohort::Native, path)).is_err());
}
