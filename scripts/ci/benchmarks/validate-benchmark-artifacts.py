# Copyright (c) 2026 Xberg. All rights reserved.
"""Validate the exact benchmark artifact contract for one fixed cohort."""
# ruff: noqa: INP001

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any


@dataclass(frozen=True)
class MatrixEntry:
    artifact: str
    framework: str
    output_format: str
    mode: str

    @property
    def aggregate_key(self) -> str:
        aggregate_mode = "single" if self.mode == "single-file" else "batch"
        if self.framework.startswith("xberg-"):
            return f"{self.framework}:{aggregate_mode}"
        return f"{self.framework}:{self.output_format}:{aggregate_mode}"


@dataclass(frozen=True)
class CohortContract:
    manifest_name: str
    manifest_blake3: str
    batch_size: int
    fixtures: tuple[str, ...]
    document_stems: tuple[str, ...]
    matrix: tuple[MatrixEntry, ...]


def matrix_entry(
    artifact: str,
    framework: str,
    output_format: str,
    mode: str,
) -> MatrixEntry:
    return MatrixEntry(artifact, framework, output_format, mode)


def xberg_entries(cohort: str) -> list[MatrixEntry]:
    return [
        matrix_entry(
            f"benchmarks-rust-{pipeline}-{output_format}-{mode}-{cohort}",
            f"xberg-{output_format}-{pipeline}",
            output_format,
            mode,
        )
        for pipeline in ("baseline", "layout")
        for output_format in ("markdown", "plaintext")
        for mode in ("single-file", "batch")
    ]


def grid_entries(framework: str, cohort: str) -> list[MatrixEntry]:
    return [
        matrix_entry(
            f"benchmarks-{framework}-{output_format}-{mode}-{cohort}",
            framework,
            output_format,
            mode,
        )
        for output_format in ("markdown", "plaintext")
        for mode in ("single-file", "batch")
    ]


def markdown_mode_entries(framework: str, cohort: str) -> list[MatrixEntry]:
    """Return native Markdown single-file and batch cells."""
    return [
        matrix_entry(
            f"benchmarks-{framework}-markdown-{mode}-{cohort}",
            framework,
            "markdown",
            mode,
        )
        for mode in ("single-file", "batch")
    ]


NATIVE_COHORT = "native-pdf-fast-b8"
OCR_COHORT = "ocr-pdf-fast-b4"

CONTRACTS = {
    "native": CohortContract(
        manifest_name="native-pdf-fast-b8-v1",
        manifest_blake3="c10d1f78d3f9d61070c0d91e7bfe90a904b69c9d3261536eb1dcff2081c73f6b",
        batch_size=8,
        fixtures=(
            "pdf_tiny_memo.json",
            "pdf_tables.json",
            "pdf_embedded.json",
            "pdf_google_docs.json",
            "pdf/681693.json",
            "pdf/ft_ACN_2009_page_102_t0.json",
            "pdf/pb_fqr-retail-blackrock-global-allocation-fund-inc_page4.json",
            "pdf/pb_sample_page_16_page1.json",
        ),
        document_stems=(
            "fake_memo",
            "tiny",
            "embedded_images_tables",
            "google_doc_document",
            "681693",
            "ft_A\x43N_2009_page_102_t0",
            "pb_fqr-retail-blackrock-global-allocation-fund-inc_page4",
            "pb_sample_page_16_page1",
        ),
        matrix=tuple(
            xberg_entries(NATIVE_COHORT)
            + grid_entries("docling", NATIVE_COHORT)
            + [
                matrix_entry(
                    f"benchmarks-markitdown-markdown-single-file-{NATIVE_COHORT}",
                    "markitdown",
                    "markdown",
                    "single-file",
                ),
                matrix_entry(
                    f"benchmarks-unstructured-markdown-single-file-{NATIVE_COHORT}",
                    "unstructured",
                    "markdown",
                    "single-file",
                ),
                matrix_entry(
                    f"benchmarks-unstructured-plaintext-single-file-{NATIVE_COHORT}",
                    "unstructured",
                    "plaintext",
                    "single-file",
                ),
                matrix_entry(
                    f"benchmarks-tika-plaintext-single-file-{NATIVE_COHORT}",
                    "tika",
                    "plaintext",
                    "single-file",
                ),
                matrix_entry(
                    f"benchmarks-pymupdf4llm-markdown-single-file-{NATIVE_COHORT}",
                    "pymupdf4llm",
                    "markdown",
                    "single-file",
                ),
            ]
            + markdown_mode_entries("mineru", NATIVE_COHORT)
            + grid_entries("liteparse", NATIVE_COHORT)
        ),
    ),
    "ocr": CohortContract(
        manifest_name="ocr-pdf-fast-b4-v1",
        manifest_blake3="c740a9480fd0ad4311a4905206345c58d7ef7a987682cc96644e2f0ffe616a13",
        batch_size=4,
        fixtures=(
            "pdf_non_searchable.json",
            "pdf_ocr_test.json",
            "pdf_scanned_ocr.json",
            "pdf_image_only_german.json",
        ),
        document_stems=("non_searchable", "ocr_test", "scanned", "image_only_german_pdf"),
        matrix=tuple(
            xberg_entries(OCR_COHORT)
            + grid_entries("docling", OCR_COHORT)
            + markdown_mode_entries("mineru", OCR_COHORT)
            + grid_entries("liteparse", OCR_COHORT)
        ),
    ),
}


class ContractError(ValueError):
    """An artifact violates the benchmark release contract."""


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"{path}: malformed or unreadable JSON: {error}") from error


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def validate_manifest(path: Path, contract: CohortContract) -> None:
    manifest = load_json(path)
    require(isinstance(manifest, dict), f"{path}: manifest must be an object")
    require(manifest.get("schema_version") == 1, f"{path}: unexpected schema_version")
    require(manifest.get("name") == contract.manifest_name, f"{path}: unexpected cohort name")
    require(manifest.get("batch_size") == contract.batch_size, f"{path}: unexpected batch_size")
    require(tuple(manifest.get("fixtures", ())) == contract.fixtures, f"{path}: fixture order/content mismatch")


def expected_document_names(fixtures_root: Path, contract: CohortContract) -> tuple[str, ...]:
    names = []
    for fixture in contract.fixtures:
        descriptor_path = fixtures_root / fixture
        descriptor = load_json(descriptor_path)
        require(isinstance(descriptor, dict), f"{descriptor_path}: fixture descriptor must be an object")
        document = descriptor.get("document")
        require(isinstance(document, str) and document, f"{descriptor_path}: document must be a non-empty string")
        names.append(PurePosixPath(document.replace("\\", "/")).name)
    require(len(set(names)) == len(names), "cohort document basenames must be unique")
    require(
        tuple(Path(name).stem for name in names) == contract.document_stems,
        "cohort document identities do not match the release contract",
    )
    return tuple(names)


def only_file(root: Path, filename: str) -> Path:
    matches = sorted(root.rglob(filename))
    require(len(matches) == 1, f"{root}: expected exactly one {filename}, found {len(matches)}")
    return matches[0]


def validate_provenance(
    provenance: Any,
    path: Path,
    *,
    entry: MatrixEntry,
    contract: CohortContract,
    source_sha: str,
    iterations: int,
) -> None:
    require(isinstance(provenance, dict), f"{path}: provenance must be an object")
    require(provenance.get("schema_version") == 2, f"{path}: unexpected provenance schema")
    repository = provenance.get("repository")
    require(isinstance(repository, dict), f"{path}: repository provenance missing")
    require(repository.get("commit") == source_sha, f"{path}: source SHA mismatch")
    require(repository.get("dirty") is False, f"{path}: benchmark checkout was dirty")

    corpus = provenance.get("corpus")
    require(isinstance(corpus, dict), f"{path}: corpus provenance missing")
    require(corpus.get("cohort") == contract.manifest_name, f"{path}: cohort name mismatch")
    require(
        corpus.get("cohort_manifest_blake3") == contract.manifest_blake3,
        f"{path}: cohort manifest hash mismatch",
    )
    ordered = corpus.get("ordered_fixtures")
    require(isinstance(ordered, list), f"{path}: ordered_fixtures must be an array")
    require(
        tuple(item.get("fixture") for item in ordered if isinstance(item, dict)) == contract.fixtures,
        f"{path}: fixture count/order mismatch",
    )
    for index, item in enumerate(ordered):
        require(isinstance(item, dict), f"{path}: fixture provenance {index} must be an object")
        for digest_name in ("fixture_blake3", "document_blake3"):
            digest = item.get(digest_name)
            require(
                isinstance(digest, str) and len(digest) == 64 and all(char in "0123456789abcdef" for char in digest),
                f"{path}: fixture {index} has malformed {digest_name}",
            )
        require(
            isinstance(item.get("document_bytes"), int) and item["document_bytes"] > 0,
            f"{path}: fixture {index} has invalid document_bytes",
        )

    timing = provenance.get("timing")
    require(isinstance(timing, dict), f"{path}: timing provenance missing")
    expected_mode = "SingleFile" if entry.mode == "single-file" else "Batch"
    require(timing.get("mode") == expected_mode, f"{path}: execution mode mismatch")
    require(timing.get("benchmark_iterations") == iterations, f"{path}: iteration count mismatch")
    require(timing.get("output_format") == entry.output_format, f"{path}: output format mismatch")
    expected_batch = contract.batch_size if entry.mode == "batch" else None
    require(provenance.get("fixed_batch_size") == expected_batch, f"{path}: fixed batch size mismatch")

    frameworks = provenance.get("frameworks")
    require(isinstance(frameworks, list) and len(frameworks) == 1, f"{path}: expected one framework")
    require(frameworks[0].get("name") == entry.framework, f"{path}: framework mismatch")
    require(frameworks[0].get("eligible_documents") == len(contract.fixtures), f"{path}: fixture count mismatch")
    expected_partitions = len(contract.fixtures) // contract.batch_size if entry.mode == "batch" else None
    require(frameworks[0].get("batch_partitions") == expected_partitions, f"{path}: batch partition mismatch")


def validate_results(
    results: Any,
    path: Path,
    *,
    entry: MatrixEntry,
    contract: CohortContract,
    document_names: tuple[str, ...],
    iterations: int,
) -> None:
    require(isinstance(results, list), f"{path}: results must be an array")
    require(len(results) == len(contract.fixtures), f"{path}: result fixture count mismatch")
    actual_names = tuple(
        PurePosixPath(str(result.get("file_path", "")).replace("\\", "/")).name
        for result in results
        if isinstance(result, dict)
    )
    require(actual_names == document_names, f"{path}: result fixture order/content mismatch")
    require(len(set(actual_names)) == len(actual_names), f"{path}: duplicate fixture results")

    expected_ocr = "used" if contract is CONTRACTS["ocr"] else "not_used"
    for index, result in enumerate(results):
        require(isinstance(result, dict), f"{path}: result {index} must be an object")
        require(result.get("framework") == entry.framework, f"{path}: result {index} framework mismatch")
        require(result.get("output_format") == entry.output_format, f"{path}: result {index} format mismatch")
        require(result.get("success") is True, f"{path}: result {index} failed")
        require(result.get("error_kind") == "none", f"{path}: result {index} has an error")
        require(result.get("error_message") is None, f"{path}: result {index} has an error message")
        require(result.get("ocr_status") == expected_ocr, f"{path}: result {index} OCR status mismatch")
        run_iterations = result.get("iterations")
        require(isinstance(run_iterations, list), f"{path}: result {index} iterations must be an array")
        require(len(run_iterations) == iterations, f"{path}: result {index} iteration count mismatch")
        require(
            [item.get("iteration") for item in run_iterations if isinstance(item, dict)] == list(range(iterations)),
            f"{path}: result {index} iteration order/duplicates mismatch",
        )


def validate_artifacts(args: argparse.Namespace, contract: CohortContract) -> None:
    validate_manifest(args.cohort_manifest, contract)
    documents = expected_document_names(args.fixtures_root, contract)
    expected_names = {f"{entry.artifact}-{args.run_id}": entry for entry in contract.matrix}
    actual_dirs = {path.name: path for path in args.artifacts_dir.iterdir() if path.is_dir()}
    require(set(actual_dirs) == set(expected_names), describe_set_mismatch("artifacts", expected_names, actual_dirs))

    for artifact_name, entry in expected_names.items():
        artifact_dir = actual_dirs[artifact_name]
        results_path = only_file(artifact_dir, "results.json")
        provenance_path = only_file(artifact_dir, "provenance.json")
        validate_provenance(
            load_json(provenance_path),
            provenance_path,
            entry=entry,
            contract=contract,
            source_sha=args.source_sha,
            iterations=args.iterations,
        )
        validate_results(
            load_json(results_path),
            results_path,
            entry=entry,
            contract=contract,
            document_names=documents,
            iterations=args.iterations,
        )
    print(f"validated {len(expected_names)} {args.cohort} benchmark artifacts")


def describe_set_mismatch(label: str, expected: Any, actual: Any) -> str:
    missing = sorted(set(expected) - set(actual))
    unexpected = sorted(set(actual) - set(expected))
    return f"{label} mismatch; missing={missing}, unexpected={unexpected}"


def validate_aggregate(args: argparse.Namespace, contract: CohortContract) -> None:
    aggregate = load_json(args.aggregated_file)
    require(isinstance(aggregate, dict), f"{args.aggregated_file}: aggregate must be an object")
    require(aggregate.get("schema_version") == "2.6.0", f"{args.aggregated_file}: unexpected schema")
    groups = aggregate.get("by_framework_mode")
    require(isinstance(groups, dict), f"{args.aggregated_file}: by_framework_mode missing")
    expected_keys = {entry.aggregate_key for entry in contract.matrix}
    require(set(groups) == expected_keys, describe_set_mismatch("aggregate keys", expected_keys, groups))
    for key, group in groups.items():
        require(isinstance(group, dict), f"{args.aggregated_file}: group {key} must be an object")
        by_file_type = group.get("by_file_type")
        require(isinstance(by_file_type, dict), f"{args.aggregated_file}: group {key} missing by_file_type")
        for file_group in by_file_type.values():
            require(isinstance(file_group, dict), f"{args.aggregated_file}: malformed file group in {key}")
            buckets = [file_group.get("no_ocr"), file_group.get("with_ocr")]
            populated = [bucket for bucket in buckets if bucket is not None]
            require(len(populated) == 1, f"{args.aggregated_file}: group {key} has ambiguous OCR buckets")
            bucket = populated[0]
            require(isinstance(bucket, dict), f"{args.aggregated_file}: malformed metrics bucket in {key}")
            require(bucket.get("total_sample_count") == len(contract.fixtures), f"{key}: fixture count mismatch")
            for error_field in (
                "framework_errors",
                "harness_errors",
                "config_setup_errors",
                "timeouts",
                "empty_content",
            ):
                require(bucket.get(error_field) == 0, f"{key}: nonzero {error_field}")

    rows = aggregate.get("per_fixture_results")
    require(isinstance(rows, list), f"{args.aggregated_file}: per_fixture_results missing")
    expected_rows = len(contract.matrix) * len(contract.fixtures)
    require(len(rows) == expected_rows, f"{args.aggregated_file}: expected {expected_rows} fixture rows")
    identities = {
        (
            row.get("framework"),
            row.get("output_format"),
            row.get("execution_mode"),
            row.get("fixture_id"),
            row.get("ocr"),
        )
        for row in rows
        if isinstance(row, dict)
    }
    expected_ocr = args.cohort == "ocr"
    expected_identities = {
        (
            entry.framework,
            entry.output_format,
            "single" if entry.mode == "single-file" else "batch",
            fixture_id,
            expected_ocr,
        )
        for entry in contract.matrix
        for fixture_id in contract.document_stems
    }
    require(
        identities == expected_identities,
        describe_set_mismatch("aggregate fixture rows", expected_identities, identities),
    )
    require(all(row.get("success") is True for row in rows), f"{args.aggregated_file}: failed fixture rows")
    print(f"validated {len(expected_keys)} {args.cohort} aggregate keys and {len(rows)} fixture rows")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cohort", choices=sorted(CONTRACTS), required=True)
    parser.add_argument("--aggregated-file", type=Path)
    parser.add_argument("--artifacts-dir", type=Path)
    parser.add_argument("--cohort-manifest", type=Path)
    parser.add_argument("--fixtures-root", type=Path)
    parser.add_argument("--source-sha")
    parser.add_argument("--run-id")
    parser.add_argument("--iterations", type=int, default=3)
    args = parser.parse_args()
    if args.aggregated_file is None:
        required = ("artifacts_dir", "cohort_manifest", "fixtures_root", "source_sha", "run_id")
        missing = [name.replace("_", "-") for name in required if getattr(args, name) in (None, "")]
        parser.error(f"artifact validation requires: {', '.join(missing)}" if missing else "")
    return args


def main() -> int:
    args = parse_args()
    try:
        contract = CONTRACTS[args.cohort]
        if args.aggregated_file is not None:
            validate_aggregate(args, contract)
        else:
            validate_artifacts(args, contract)
    except (ContractError, OSError) as error:
        print(f"benchmark artifact validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
