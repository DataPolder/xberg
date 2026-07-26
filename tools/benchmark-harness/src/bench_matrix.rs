//! Pinned benchmark artifact release contract: cohort manifests, the matrix of expected
//! framework x output-format x execution-mode cells, and their derived aggregate keys.
//!
//! Ported from `scripts/ci/benchmarks/validate-benchmark-artifacts.py`. The digests and matrix
//! shape below are pinned release-contract values copied verbatim from that script; they must
//! only change alongside a deliberate cohort/matrix revision, never as an incidental refactor.

use crate::aggregate::make_aggregate_key;
use crate::config::BenchmarkMode;
use crate::types::OutputFormat;
use crate::{Error, Result};

/// Execution mode of one matrix cell, and the three string encodings the release contract
/// expects it to appear as (artifact-directory slug, aggregate-key slug, provenance enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Sequential single-document execution.
    SingleFile,
    /// Concurrent/native-batch execution.
    Batch,
}

impl ExecutionMode {
    /// Slug used in artifact directory names, e.g. `benchmarks-docling-markdown-single-file-<cohort>`.
    pub fn artifact_slug(self) -> &'static str {
        match self {
            ExecutionMode::SingleFile => "single-file",
            ExecutionMode::Batch => "batch",
        }
    }

    /// Slug used in `by_framework_mode` aggregate keys and `execution_mode` fixture rows.
    pub fn aggregate_slug(self) -> &'static str {
        match self {
            ExecutionMode::SingleFile => "single",
            ExecutionMode::Batch => "batch",
        }
    }

    /// The `TimingProvenance::mode` value a run in this execution mode must record.
    pub fn benchmark_mode(self) -> BenchmarkMode {
        match self {
            ExecutionMode::SingleFile => BenchmarkMode::SingleFile,
            ExecutionMode::Batch => BenchmarkMode::Batch,
        }
    }
}

/// One expected artifact cell in a cohort's release contract.
#[derive(Debug, Clone)]
pub struct MatrixEntry {
    /// Artifact directory name, without the trailing `-<run-id>` suffix.
    pub artifact: String,
    /// Framework name this cell was produced by.
    pub framework: String,
    /// Output format this cell was produced with.
    pub output_format: OutputFormat,
    /// Execution mode this cell was produced with.
    pub mode: ExecutionMode,
    /// When true, this cell is best-effort: its absence must not fail validation and it is
    /// excluded from the required release contract (e.g. MinerU, whose offline model-config
    /// fetch can hang). A present, well-formed optional artifact still flows into the
    /// aggregate; the validator simply does not gate publication on it.
    pub optional: bool,
}

impl MatrixEntry {
    /// Derive this cell's `by_framework_mode` aggregate key by delegating to the same key
    /// builder the `consolidate` command uses, so the contract can never drift from it.
    pub fn aggregate_key(&self) -> String {
        make_aggregate_key(&self.framework, self.output_format, self.mode.aggregate_slug())
    }

    /// Mark this cell best-effort (non-contractual). See [`MatrixEntry::optional`].
    pub fn into_optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

/// The exact, pinned artifact contract for one benchmark cohort.
#[derive(Debug, Clone)]
pub struct CohortContract {
    /// Cohort manifest `name` field (e.g. `native-pdf-fast-b8-v1`).
    pub manifest_name: &'static str,
    /// Pinned BLAKE3 digest of the cohort manifest file's exact bytes.
    pub manifest_blake3: &'static str,
    /// Required native-batch document count.
    pub batch_size: usize,
    /// Fixture descriptor paths, in the exact pinned order.
    pub fixtures: &'static [&'static str],
    /// Expected document basename stems, in fixture order.
    pub document_stems: &'static [&'static str],
    /// Every expected framework x output-format x execution-mode cell.
    pub matrix: Vec<MatrixEntry>,
}

/// Which cohort's release contract is being validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cohort {
    /// Native (non-OCR) fast PDF cohort.
    Native,
    /// OCR fast PDF cohort.
    Ocr,
}

impl Cohort {
    /// The `--cohort` CLI value for this cohort.
    pub fn as_str(self) -> &'static str {
        match self {
            Cohort::Native => "native",
            Cohort::Ocr => "ocr",
        }
    }

    /// Parse a `--cohort` CLI value.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "native" => Ok(Cohort::Native),
            "ocr" => Ok(Cohort::Ocr),
            other => Err(Error::Config(format!(
                "unknown cohort '{other}': expected one of: native, ocr"
            ))),
        }
    }

    /// Whether every result in this cohort's artifacts is expected to have used OCR.
    pub fn expects_ocr(self) -> bool {
        matches!(self, Cohort::Ocr)
    }

    /// Build this cohort's pinned release contract.
    pub fn contract(self) -> CohortContract {
        match self {
            Cohort::Native => native_contract(),
            Cohort::Ocr => ocr_contract(),
        }
    }
}

const NATIVE_COHORT: &str = "native-pdf-fast-b8";
const OCR_COHORT: &str = "ocr-pdf-fast-b4";

const NATIVE_MANIFEST_NAME: &str = "native-pdf-fast-b8-v1";
const NATIVE_MANIFEST_BLAKE3: &str = "c10d1f78d3f9d61070c0d91e7bfe90a904b69c9d3261536eb1dcff2081c73f6b";
const NATIVE_BATCH_SIZE: usize = 8;
const NATIVE_FIXTURES: &[&str] = &[
    "pdf_tiny_memo.json",
    "pdf_tables.json",
    "pdf_embedded.json",
    "pdf_google_docs.json",
    "pdf/681693.json",
    "pdf/ft_ACN_2009_page_102_t0.json",
    "pdf/pb_fqr-retail-blackrock-global-allocation-fund-inc_page4.json",
    "pdf/pb_sample_page_16_page1.json",
];
const NATIVE_DOCUMENT_STEMS: &[&str] = &[
    "fake_memo",
    "tiny",
    "embedded_images_tables",
    "google_doc_document",
    "681693",
    "ft_ACN_2009_page_102_t0",
    "pb_fqr-retail-blackrock-global-allocation-fund-inc_page4",
    "pb_sample_page_16_page1",
];

const OCR_MANIFEST_NAME: &str = "ocr-pdf-fast-b4-v1";
const OCR_MANIFEST_BLAKE3: &str = "f9e4e881b70111df10516a5f2cf2ed648f67b299f9116bafb34746e98436b66b";
const OCR_BATCH_SIZE: usize = 4;
const OCR_FIXTURES: &[&str] = &[
    "pdf_non_searchable.json",
    "pdf_ocr_test.json",
    "pdf_scanned_ocr.json",
    "pdf_image_only_german.json",
];
const OCR_DOCUMENT_STEMS: &[&str] = &["non_searchable", "ocr_test", "scanned", "image_only_german_pdf"];

fn matrix_entry(
    artifact: String,
    framework: impl Into<String>,
    output_format: OutputFormat,
    mode: ExecutionMode,
) -> MatrixEntry {
    MatrixEntry {
        artifact,
        framework: framework.into(),
        output_format,
        mode,
        optional: false,
    }
}

/// The four native Xberg cells (baseline/layout pipelines x markdown/plaintext x single/batch).
fn xberg_entries(cohort: &str) -> Vec<MatrixEntry> {
    let mut entries = Vec::new();
    for pipeline in ["baseline", "layout"] {
        for output_format in [OutputFormat::Markdown, OutputFormat::Plaintext] {
            for mode in [ExecutionMode::SingleFile, ExecutionMode::Batch] {
                entries.push(matrix_entry(
                    format!(
                        "benchmarks-rust-{pipeline}-{output_format}-{}-{cohort}",
                        mode.artifact_slug()
                    ),
                    format!("xberg-{output_format}-{pipeline}"),
                    output_format,
                    mode,
                ));
            }
        }
    }
    entries
}

/// The full markdown/plaintext x single/batch grid for one competitor framework.
fn grid_entries(framework: &str, cohort: &str) -> Vec<MatrixEntry> {
    let mut entries = Vec::new();
    for output_format in [OutputFormat::Markdown, OutputFormat::Plaintext] {
        for mode in [ExecutionMode::SingleFile, ExecutionMode::Batch] {
            entries.push(matrix_entry(
                format!(
                    "benchmarks-{framework}-{output_format}-{}-{cohort}",
                    mode.artifact_slug()
                ),
                framework,
                output_format,
                mode,
            ));
        }
    }
    entries
}

/// The single native Markdown/single-file cell for one framework (e.g. MinerU, which never
/// runs in native-batch mode).
fn markdown_single_file_entry(framework: &str, cohort: &str) -> MatrixEntry {
    matrix_entry(
        format!("benchmarks-{framework}-markdown-single-file-{cohort}"),
        framework,
        OutputFormat::Markdown,
        ExecutionMode::SingleFile,
    )
}

fn native_matrix() -> Vec<MatrixEntry> {
    let mut matrix = xberg_entries(NATIVE_COHORT);
    matrix.extend(grid_entries("docling", NATIVE_COHORT));
    matrix.push(matrix_entry(
        format!("benchmarks-markitdown-markdown-single-file-{NATIVE_COHORT}"),
        "markitdown",
        OutputFormat::Markdown,
        ExecutionMode::SingleFile,
    ));
    matrix.push(matrix_entry(
        format!("benchmarks-unstructured-plaintext-single-file-{NATIVE_COHORT}"),
        "unstructured",
        OutputFormat::Plaintext,
        ExecutionMode::SingleFile,
    ));
    matrix.push(matrix_entry(
        format!("benchmarks-tika-plaintext-single-file-{NATIVE_COHORT}"),
        "tika",
        OutputFormat::Plaintext,
        ExecutionMode::SingleFile,
    ));
    matrix.push(matrix_entry(
        format!("benchmarks-pymupdf4llm-markdown-single-file-{NATIVE_COHORT}"),
        "pymupdf4llm",
        OutputFormat::Markdown,
        ExecutionMode::SingleFile,
    ));
    matrix.push(markdown_single_file_entry("mineru", NATIVE_COHORT).into_optional());
    matrix.extend(grid_entries("liteparse", NATIVE_COHORT));
    matrix
}

fn ocr_matrix() -> Vec<MatrixEntry> {
    let mut matrix = xberg_entries(OCR_COHORT);
    matrix.extend(grid_entries("docling", OCR_COHORT));
    matrix.push(markdown_single_file_entry("mineru", OCR_COHORT).into_optional());
    matrix.extend(grid_entries("liteparse", OCR_COHORT));
    matrix
}

fn native_contract() -> CohortContract {
    CohortContract {
        manifest_name: NATIVE_MANIFEST_NAME,
        manifest_blake3: NATIVE_MANIFEST_BLAKE3,
        batch_size: NATIVE_BATCH_SIZE,
        fixtures: NATIVE_FIXTURES,
        document_stems: NATIVE_DOCUMENT_STEMS,
        matrix: native_matrix(),
    }
}

fn ocr_contract() -> CohortContract {
    CohortContract {
        manifest_name: OCR_MANIFEST_NAME,
        manifest_blake3: OCR_MANIFEST_BLAKE3,
        batch_size: OCR_BATCH_SIZE,
        fixtures: OCR_FIXTURES,
        document_stems: OCR_DOCUMENT_STEMS,
        matrix: ocr_matrix(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn native_contract_key_counts_are_exact() {
        let contract = Cohort::Native.contract();
        assert_eq!(contract.matrix.len(), 21);
        assert_eq!(
            contract
                .matrix
                .iter()
                .map(|entry| &entry.artifact)
                .collect::<HashSet<_>>()
                .len(),
            contract.matrix.len()
        );
        assert_eq!(
            contract
                .matrix
                .iter()
                .map(MatrixEntry::aggregate_key)
                .collect::<HashSet<_>>()
                .len(),
            contract.matrix.len()
        );
    }

    #[test]
    fn ocr_contract_key_counts_are_exact() {
        let contract = Cohort::Ocr.contract();
        assert_eq!(contract.matrix.len(), 17);
        assert_eq!(
            contract
                .matrix
                .iter()
                .map(|entry| &entry.artifact)
                .collect::<HashSet<_>>()
                .len(),
            contract.matrix.len()
        );
        assert_eq!(
            contract
                .matrix
                .iter()
                .map(MatrixEntry::aggregate_key)
                .collect::<HashSet<_>>()
                .len(),
            contract.matrix.len()
        );
    }

    #[test]
    fn mineru_is_single_file_only_in_every_cohort() {
        for cohort in [Cohort::Native, Cohort::Ocr] {
            let contract = cohort.contract();
            let mineru_modes: Vec<ExecutionMode> = contract
                .matrix
                .iter()
                .filter(|entry| entry.framework == "mineru")
                .map(|entry| entry.mode)
                .collect();
            assert_eq!(mineru_modes, vec![ExecutionMode::SingleFile]);
        }
    }

    #[test]
    fn cohort_parse_round_trips_as_str() {
        assert_eq!(Cohort::parse("native").unwrap(), Cohort::Native);
        assert_eq!(Cohort::parse("ocr").unwrap(), Cohort::Ocr);
        assert!(Cohort::parse("bogus").is_err());
        assert_eq!(Cohort::Native.as_str(), "native");
        assert_eq!(Cohort::Ocr.as_str(), "ocr");
    }

    #[test]
    fn xberg_aggregate_keys_omit_output_format() {
        let contract = Cohort::Native.contract();
        let entry = contract
            .matrix
            .iter()
            .find(|entry| entry.framework == "xberg-markdown-baseline")
            .expect("xberg-markdown-baseline cell present");
        assert_eq!(entry.aggregate_key(), "xberg-markdown-baseline:single");
    }
}
