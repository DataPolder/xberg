//! Validate the exact benchmark artifact contract for one fixed cohort.
//!
//! Ported from `scripts/ci/benchmarks/validate-benchmark-artifacts.py`. Two modes are
//! supported, matching the Python CLI:
//!
//! - **artifact mode** (`aggregated_file` is `None`): validates one directory of raw
//!   per-framework `run/{provenance,results}.json` artifacts against [`crate::bench_matrix`]'s
//!   pinned cohort contract.
//! - **aggregate mode** (`aggregated_file` is `Some`): validates one consolidated
//!   `aggregated.json` (as produced by the `consolidate` subcommand) against the same contract.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::aggregate::{
    NewConsolidatedResults, PerFixtureRow, PerformancePercentiles, SCHEMA_VERSION, extract_framework_and_mode,
};
use crate::bench_matrix::{Cohort, CohortContract, ExecutionMode, MatrixEntry};
use crate::provenance::RunProvenance;
use crate::types::{BenchmarkResult, ErrorKind, OcrStatus, OutputFormat};
use crate::{Error, Result};

/// Provenance schema version every `provenance.json` must record.
///
/// Mirrors the private `PROVENANCE_SCHEMA_VERSION` constant in [`crate::provenance`]; kept as a
/// named constant here (rather than a literal `2`) because that constant is not exported.
const EXPECTED_PROVENANCE_SCHEMA_VERSION: u32 = 2;

/// Inputs for [`validate`], mirroring the Python script's `argparse` surface.
#[derive(Debug, Clone)]
pub struct ValidateArtifactsArgs {
    /// Which cohort's release contract to validate against.
    pub cohort: Cohort,
    /// Path to a consolidated `aggregated.json`. When set, aggregate mode runs instead of
    /// artifact mode and every other field below is ignored.
    pub aggregated_file: Option<PathBuf>,
    /// Directory containing one subdirectory per expected artifact (artifact mode only).
    pub artifacts_dir: Option<PathBuf>,
    /// Path to the pinned cohort manifest JSON (artifact mode only).
    pub cohort_manifest: Option<PathBuf>,
    /// Root directory fixture paths in the manifest are resolved against (artifact mode only).
    pub fixtures_root: Option<PathBuf>,
    /// Benchmark source revision every `provenance.json` must record (artifact mode only).
    pub source_sha: Option<String>,
    /// Run identifier suffix shared by every expected artifact directory (artifact mode only).
    pub run_id: Option<String>,
    /// Benchmark iterations every `provenance.json`/`results.json` must record.
    pub iterations: usize,
}

/// Validate one cohort's benchmark artifact contract, dispatching to artifact or aggregate mode.
///
/// Returns a human-readable summary line on success, matching the Python script's stdout.
pub fn validate(args: &ValidateArtifactsArgs) -> Result<String> {
    let contract = args.cohort.contract();
    match &args.aggregated_file {
        Some(aggregated_file) => validate_aggregate(aggregated_file, args.cohort, &contract),
        None => {
            let (artifacts_dir, cohort_manifest, fixtures_root, source_sha, run_id) = require_artifact_args(args)?;
            validate_raw_artifacts(
                artifacts_dir,
                cohort_manifest,
                fixtures_root,
                source_sha,
                run_id,
                args.iterations,
                args.cohort,
                &contract,
            )
        }
    }
}

fn require_artifact_args(args: &ValidateArtifactsArgs) -> Result<(&Path, &Path, &Path, &str, &str)> {
    let mut missing = Vec::new();
    if args.artifacts_dir.is_none() {
        missing.push("artifacts-dir");
    }
    if args.cohort_manifest.is_none() {
        missing.push("cohort-manifest");
    }
    if args.fixtures_root.is_none() {
        missing.push("fixtures-root");
    }
    if args.source_sha.as_deref().unwrap_or_default().is_empty() {
        missing.push("source-sha");
    }
    if args.run_id.as_deref().unwrap_or_default().is_empty() {
        missing.push("run-id");
    }
    if !missing.is_empty() {
        return Err(Error::Config(format!(
            "artifact validation requires: {}",
            missing.join(", ")
        )));
    }
    Ok((
        args.artifacts_dir.as_deref().expect("checked above"),
        args.cohort_manifest.as_deref().expect("checked above"),
        args.fixtures_root.as_deref().expect("checked above"),
        args.source_sha.as_deref().expect("checked above"),
        args.run_id.as_deref().expect("checked above"),
    ))
}

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Benchmark(message.into()))
    }
}

fn contract_error(message: impl Into<String>) -> Error {
    Error::Benchmark(message.into())
}

/// One fixture's expected identity, derived from the real fixture descriptor and document bytes
/// referenced by the pinned cohort contract.
struct ExpectedFixture {
    fixture: String,
    fixture_blake3: String,
    document_blake3: String,
    document_bytes: u64,
    document_name: String,
}

/// Compute a BLAKE3 digest identical to the `b3sum` CLI output the Python script shelled out to.
fn blake3_file(path: &Path) -> Result<String> {
    if !path.is_file() {
        return Err(contract_error(format!("{}: expected a regular file", path.display())));
    }
    let mut file = std::fs::File::open(path).map_err(Error::Io)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(Error::Io)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn load_json_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|error| contract_error(format!("{}: malformed or unreadable JSON: {error}", path.display())))
}

fn load_json_value(path: &Path) -> Result<serde_json::Value> {
    let text = load_json_text(path)?;
    serde_json::from_str(&text)
        .map_err(|error| contract_error(format!("{}: malformed or unreadable JSON: {error}", path.display())))
}

fn load_typed_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = load_json_text(path)?;
    serde_json::from_str(&text)
        .map_err(|error| contract_error(format!("{}: malformed or unreadable JSON: {error}", path.display())))
}

/// Basename of a `/`- or `\`-separated path string, matching `PurePosixPath(...).name`.
fn posix_basename(raw: &str) -> String {
    raw.replace('\\', "/").rsplit('/').next().unwrap_or(raw).to_string()
}

fn describe_set_mismatch<'a>(
    label: &str,
    expected: impl Iterator<Item = &'a str>,
    actual: impl Iterator<Item = &'a str>,
) -> String {
    let expected: HashSet<&str> = expected.collect();
    let actual: HashSet<&str> = actual.collect();
    let mut missing: Vec<&str> = expected.difference(&actual).copied().collect();
    missing.sort_unstable();
    let mut unexpected: Vec<&str> = actual.difference(&expected).copied().collect();
    unexpected.sort_unstable();
    format!("{label} mismatch; missing={missing:?}, unexpected={unexpected:?}")
}

fn validate_manifest(path: &Path, contract: &CohortContract) -> Result<String> {
    let manifest = load_json_value(path)?;
    let object = manifest
        .as_object()
        .ok_or_else(|| contract_error(format!("{}: manifest must be an object", path.display())))?;
    require(
        object.get("schema_version").and_then(serde_json::Value::as_u64) == Some(1),
        format!("{}: unexpected schema_version", path.display()),
    )?;
    require(
        object.get("name").and_then(serde_json::Value::as_str) == Some(contract.manifest_name),
        format!("{}: unexpected cohort name", path.display()),
    )?;
    require(
        object.get("batch_size").and_then(serde_json::Value::as_u64) == Some(contract.batch_size as u64),
        format!("{}: unexpected batch_size", path.display()),
    )?;
    let fixtures = object
        .get("fixtures")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| contract_error(format!("{}: fixtures must be an array", path.display())))?;
    let fixtures_match = fixtures
        .iter()
        .map(serde_json::Value::as_str)
        .collect::<Option<Vec<&str>>>()
        .is_some_and(|values| values == contract.fixtures);
    require(
        fixtures_match,
        format!("{}: fixture order/content mismatch", path.display()),
    )?;
    let digest = blake3_file(path)?;
    require(
        digest == contract.manifest_blake3,
        format!("{}: cohort manifest BLAKE3 mismatch", path.display()),
    )?;
    Ok(digest)
}

fn expected_fixtures(fixtures_root: &Path, contract: &CohortContract) -> Result<Vec<ExpectedFixture>> {
    let mut expected = Vec::with_capacity(contract.fixtures.len());
    for fixture in contract.fixtures {
        let descriptor_path = fixtures_root.join(fixture);
        let descriptor = load_json_value(&descriptor_path)?;
        let document = descriptor
            .as_object()
            .and_then(|object| object.get("document"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                contract_error(format!(
                    "{}: document must be a non-empty string",
                    descriptor_path.display()
                ))
            })?;
        let document_path = descriptor_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(document);
        let document_name = posix_basename(document);
        expected.push(ExpectedFixture {
            fixture: (*fixture).to_string(),
            fixture_blake3: blake3_file(&descriptor_path)?,
            document_blake3: blake3_file(&document_path)?,
            document_bytes: std::fs::metadata(&document_path).map_err(Error::Io)?.len(),
            document_name,
        });
    }

    let names: Vec<&str> = expected.iter().map(|item| item.document_name.as_str()).collect();
    let unique_names: HashSet<&str> = names.iter().copied().collect();
    require(
        unique_names.len() == names.len(),
        "cohort document basenames must be unique",
    )?;

    let stems: Vec<&str> = names
        .iter()
        .map(|name| {
            Path::new(name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(name)
        })
        .collect();
    require(
        stems == contract.document_stems,
        "cohort document identities do not match the release contract",
    )?;

    Ok(expected)
}

fn read_artifact_dirs(root: &Path) -> Result<std::collections::HashMap<String, PathBuf>> {
    let mut dirs = std::collections::HashMap::new();
    for entry in std::fs::read_dir(root).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        let path = entry.path();
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|name| name.to_str())
        {
            dirs.insert(name.to_string(), path);
        }
    }
    Ok(dirs)
}

fn only_file(root: &Path, filename: &str) -> Result<PathBuf> {
    let mut matches = Vec::new();
    collect_matching_files(root, filename, &mut matches)?;
    matches.sort();
    if matches.len() != 1 {
        return Err(contract_error(format!(
            "{}: expected exactly one {filename}, found {}",
            root.display(),
            matches.len()
        )));
    }
    Ok(matches.remove(0))
}

fn collect_matching_files(dir: &Path, filename: &str, matches: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_matching_files(&path, filename, matches)?;
        } else if path.file_name().and_then(|name| name.to_str()) == Some(filename) {
            matches.push(path);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_provenance(
    provenance: &RunProvenance,
    path: &Path,
    entry: &MatrixEntry,
    contract: &CohortContract,
    manifest_blake3: &str,
    expected: &[ExpectedFixture],
    source_sha: &str,
    iterations: usize,
) -> Result<()> {
    require(
        provenance.schema_version == EXPECTED_PROVENANCE_SCHEMA_VERSION,
        format!("{}: unexpected provenance schema", path.display()),
    )?;
    require(
        provenance.repository.commit.as_deref() == Some(source_sha),
        format!("{}: source SHA mismatch", path.display()),
    )?;
    require(
        provenance.repository.dirty == Some(false),
        format!("{}: benchmark checkout was dirty", path.display()),
    )?;

    require(
        provenance.corpus.cohort.as_deref() == Some(contract.manifest_name),
        format!("{}: cohort name mismatch", path.display()),
    )?;
    require(
        provenance.corpus.cohort_manifest_blake3.as_deref() == Some(manifest_blake3),
        format!("{}: cohort manifest hash mismatch", path.display()),
    )?;

    require(
        provenance.corpus.ordered_fixtures.len() == expected.len(),
        format!("{}: fixture count mismatch", path.display()),
    )?;
    for (index, (item, expected_item)) in provenance.corpus.ordered_fixtures.iter().zip(expected).enumerate() {
        require(
            item.fixture == expected_item.fixture,
            format!("{}: fixture {index} identity/order mismatch", path.display()),
        )?;
        require(
            item.fixture_blake3 == expected_item.fixture_blake3,
            format!("{}: fixture {index} descriptor BLAKE3 mismatch", path.display()),
        )?;
        require(
            item.document_blake3 == expected_item.document_blake3,
            format!("{}: fixture {index} document BLAKE3 mismatch", path.display()),
        )?;
        require(
            item.document_bytes == expected_item.document_bytes,
            format!("{}: fixture {index} document size mismatch", path.display()),
        )?;
    }

    require(
        provenance.timing.mode == entry.mode.benchmark_mode(),
        format!("{}: execution mode mismatch", path.display()),
    )?;
    require(
        provenance.timing.benchmark_iterations == iterations,
        format!("{}: iteration count mismatch", path.display()),
    )?;
    require(
        provenance.timing.output_format == entry.output_format,
        format!("{}: output format mismatch", path.display()),
    )?;

    let expected_batch = matches!(entry.mode, ExecutionMode::Batch).then_some(contract.batch_size);
    require(
        provenance.fixed_batch_size == expected_batch,
        format!("{}: fixed batch size mismatch", path.display()),
    )?;

    require(
        provenance.frameworks.len() == 1,
        format!("{}: expected one framework", path.display()),
    )?;
    let framework = &provenance.frameworks[0];
    // The run side bakes the execution mode into xberg framework names (batch cells emit
    // `xberg-<fmt>-<pipeline>-batch`), matching how `extract_framework_and_mode` recovers the
    // base name during aggregation; `entry.framework` is the mode-independent base. Compare the
    // stripped base — mode itself is validated separately via `timing.mode`/`fixed_batch_size`.
    require(
        extract_framework_and_mode(&framework.name).0 == entry.framework,
        format!("{}: framework mismatch", path.display()),
    )?;
    require(
        framework.eligible_documents == contract.fixtures.len(),
        format!("{}: fixture count mismatch", path.display()),
    )?;
    let expected_partitions =
        matches!(entry.mode, ExecutionMode::Batch).then(|| contract.fixtures.len() / contract.batch_size);
    require(
        framework.batch_partitions == expected_partitions,
        format!("{}: batch partition mismatch", path.display()),
    )?;

    Ok(())
}

fn validate_results(
    results: &[BenchmarkResult],
    path: &Path,
    entry: &MatrixEntry,
    contract: &CohortContract,
    document_names: &[String],
    iterations: usize,
    cohort: Cohort,
) -> Result<()> {
    require(
        results.len() == contract.fixtures.len(),
        format!("{}: result fixture count mismatch", path.display()),
    )?;

    let actual_names: Vec<String> = results
        .iter()
        .map(|result| posix_basename(&result.file_path.to_string_lossy()))
        .collect();
    require(
        actual_names == document_names,
        format!("{}: result fixture order/content mismatch", path.display()),
    )?;
    let unique_names: HashSet<&String> = actual_names.iter().collect();
    require(
        unique_names.len() == actual_names.len(),
        format!("{}: duplicate fixture results", path.display()),
    )?;

    let expected_ocr = if cohort.expects_ocr() {
        OcrStatus::Used
    } else {
        OcrStatus::NotUsed
    };
    for (index, result) in results.iter().enumerate() {
        require(
            extract_framework_and_mode(&result.framework).0 == entry.framework,
            format!("{}: result {index} framework mismatch", path.display()),
        )?;
        require(
            result.output_format == entry.output_format,
            format!("{}: result {index} format mismatch", path.display()),
        )?;
        require(result.success, format!("{}: result {index} failed", path.display()))?;
        require(
            result.error_kind == ErrorKind::None,
            format!("{}: result {index} has an error", path.display()),
        )?;
        require(
            result.error_message.is_none(),
            format!("{}: result {index} has an error message", path.display()),
        )?;
        require(
            result.ocr_status == expected_ocr,
            format!("{}: result {index} OCR status mismatch", path.display()),
        )?;
        require(
            result.iterations.len() == iterations,
            format!("{}: result {index} iteration count mismatch", path.display()),
        )?;
        let sequential = result
            .iterations
            .iter()
            .enumerate()
            .all(|(expected_index, iteration)| iteration.iteration == expected_index + 1);
        require(
            sequential,
            format!("{}: result {index} iteration order/duplicates mismatch", path.display()),
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_raw_artifacts(
    artifacts_dir: &Path,
    cohort_manifest: &Path,
    fixtures_root: &Path,
    source_sha: &str,
    run_id: &str,
    iterations: usize,
    cohort: Cohort,
    contract: &CohortContract,
) -> Result<String> {
    let manifest_blake3 = validate_manifest(cohort_manifest, contract)?;
    let fixtures = expected_fixtures(fixtures_root, contract)?;
    let documents: Vec<String> = fixtures.iter().map(|item| item.document_name.clone()).collect();

    let expected_names: std::collections::HashMap<String, &MatrixEntry> = contract
        .matrix
        .iter()
        .filter(|entry| !entry.optional)
        .map(|entry| (format!("{}-{run_id}", entry.artifact), entry))
        .collect();
    // Best-effort (optional) frameworks are not part of the required contract: their
    // artifact directories are neither required to be present nor rejected as unexpected.
    let optional_names: HashSet<String> = contract
        .matrix
        .iter()
        .filter(|entry| entry.optional)
        .map(|entry| format!("{}-{run_id}", entry.artifact))
        .collect();
    let actual_dirs = read_artifact_dirs(artifacts_dir)?;

    let expected_key_set: HashSet<&str> = expected_names.keys().map(String::as_str).collect();
    let actual_key_set: HashSet<&str> = actual_dirs
        .keys()
        .map(String::as_str)
        .filter(|name| !optional_names.contains(*name))
        .collect();
    require(
        expected_key_set == actual_key_set,
        describe_set_mismatch(
            "artifacts",
            expected_key_set.iter().copied(),
            actual_key_set.iter().copied(),
        ),
    )?;

    for (artifact_name, entry) in &expected_names {
        let artifact_dir = &actual_dirs[artifact_name];
        let results_path = only_file(artifact_dir, "results.json")?;
        let provenance_path = only_file(artifact_dir, "provenance.json")?;

        let provenance: RunProvenance = load_typed_json(&provenance_path)?;
        validate_provenance(
            &provenance,
            &provenance_path,
            entry,
            contract,
            &manifest_blake3,
            &fixtures,
            source_sha,
            iterations,
        )?;

        let results: Vec<BenchmarkResult> = load_typed_json(&results_path)?;
        validate_results(&results, &results_path, entry, contract, &documents, iterations, cohort)?;
    }

    Ok(format!(
        "validated {} {} benchmark artifacts",
        expected_names.len(),
        cohort.as_str()
    ))
}

/// Validate one file-type bucket has no errors and report its sample count. Cohorts can span
/// several file types (e.g. the office family: docx, pptx, xlsx), so each fixture lands in its own
/// file-type bucket; the caller sums the counts and checks the total against `fixtures.len()`.
fn validate_bucket(bucket: &PerformancePercentiles, key: &str) -> Result<usize> {
    require(bucket.framework_errors == 0, format!("{key}: nonzero framework_errors"))?;
    require(bucket.harness_errors == 0, format!("{key}: nonzero harness_errors"))?;
    require(
        bucket.config_setup_errors == 0,
        format!("{key}: nonzero config_setup_errors"),
    )?;
    require(bucket.timeouts == 0, format!("{key}: nonzero timeouts"))?;
    require(bucket.empty_content == 0, format!("{key}: nonzero empty_content"))?;
    Ok(bucket.total_sample_count)
}

fn identity_string(
    framework: &str,
    output_format: OutputFormat,
    mode: &str,
    fixture_id: &str,
    ocr: Option<bool>,
) -> String {
    format!("{framework}:{output_format}:{mode}:{fixture_id}:ocr={ocr:?}")
}

fn validate_aggregate(path: &Path, cohort: Cohort, contract: &CohortContract) -> Result<String> {
    let aggregate: NewConsolidatedResults = load_typed_json(path)?;
    require(
        aggregate.schema_version == SCHEMA_VERSION,
        format!("{}: unexpected schema", path.display()),
    )?;

    // Best-effort (optional) frameworks are excluded from the required contract on both
    // sides of every comparison: they may be absent, and if present they still ship in the
    // aggregate but do not gate validation (e.g. a failed/hung MinerU run).
    let required_entries: Vec<&MatrixEntry> = contract.matrix.iter().filter(|entry| !entry.optional).collect();
    let optional_agg_keys: HashSet<String> = contract
        .matrix
        .iter()
        .filter(|entry| entry.optional)
        .map(MatrixEntry::aggregate_key)
        .collect();
    let optional_frameworks: HashSet<&str> = contract
        .matrix
        .iter()
        .filter(|entry| entry.optional)
        .map(|entry| entry.framework.as_str())
        .collect();

    let expected_keys: HashSet<String> = required_entries.iter().map(|entry| entry.aggregate_key()).collect();
    let actual_keys: HashSet<String> = aggregate
        .by_framework_mode
        .keys()
        .filter(|key| !optional_agg_keys.contains(*key))
        .cloned()
        .collect();
    require(
        expected_keys == actual_keys,
        describe_set_mismatch(
            "aggregate keys",
            expected_keys.iter().map(String::as_str),
            actual_keys.iter().map(String::as_str),
        ),
    )?;

    let expects_ocr = cohort.expects_ocr();
    for (key, group) in &aggregate.by_framework_mode {
        if optional_agg_keys.contains(key) {
            continue;
        }
        require(
            !group.by_file_type.is_empty(),
            format!("{}: group {key} has no file-type metrics", path.display()),
        )?;
        // A cohort's OCR expectation is uniform, so every fixture in every file-type bucket must
        // sit on the same OCR side; the opposite side must be empty. Sum the per-bucket sample
        // counts and require the total to equal the cohort's fixture count.
        let mut total_samples = 0usize;
        for file_group in group.by_file_type.values() {
            let (present, absent) = if expects_ocr {
                (&file_group.with_ocr, &file_group.no_ocr)
            } else {
                (&file_group.no_ocr, &file_group.with_ocr)
            };
            require(
                absent.is_none(),
                format!(
                    "{}: group {key} file type {} has wrong OCR bucket",
                    path.display(),
                    file_group.file_type
                ),
            )?;
            let bucket = present.as_ref().ok_or_else(|| {
                contract_error(format!(
                    "{}: group {key} file type {} missing OCR bucket",
                    path.display(),
                    file_group.file_type
                ))
            })?;
            total_samples += validate_bucket(bucket, key)?;
        }
        require(
            total_samples == contract.fixtures.len(),
            format!(
                "{}: group {key} covers {total_samples} samples, expected {}",
                path.display(),
                contract.fixtures.len()
            ),
        )?;
    }

    let rows: Vec<&PerFixtureRow> = aggregate
        .per_fixture_results
        .iter()
        .filter(|row| !optional_frameworks.contains(row.framework.as_str()))
        .collect();
    let expected_row_count = required_entries.len() * contract.fixtures.len();
    require(
        rows.len() == expected_row_count,
        format!("{}: expected {expected_row_count} fixture rows", path.display()),
    )?;

    let identities: HashSet<String> = rows
        .iter()
        .map(|row| {
            identity_string(
                &row.framework,
                row.output_format,
                &row.execution_mode,
                &row.fixture_id,
                row.ocr,
            )
        })
        .collect();
    let expected_identities: HashSet<String> = required_entries
        .iter()
        .flat_map(|entry| {
            contract.document_stems.iter().map(move |stem| {
                identity_string(
                    &entry.framework,
                    entry.output_format,
                    entry.mode.aggregate_slug(),
                    stem,
                    Some(expects_ocr),
                )
            })
        })
        .collect();
    require(
        identities == expected_identities,
        describe_set_mismatch(
            "aggregate fixture rows",
            expected_identities.iter().map(String::as_str),
            identities.iter().map(String::as_str),
        ),
    )?;

    require(
        rows.iter().all(|row| row.success),
        format!("{}: failed fixture rows", path.display()),
    )?;

    Ok(format!(
        "validated {} {} aggregate keys and {} fixture rows",
        expected_keys.len(),
        cohort.as_str(),
        rows.len()
    ))
}
