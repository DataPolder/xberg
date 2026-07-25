# Copyright (c) 2026 Xberg. All rights reserved.
"""Tests for the remote benchmark artifact contract validator."""
# ruff: noqa: D101, D102, PT009, PT027

from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/ci/benchmarks/validate-benchmark-artifacts.py"
SPEC = importlib.util.spec_from_file_location("validate_benchmark_artifacts", SCRIPT)
assert SPEC and SPEC.loader
validator = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = validator
SPEC.loader.exec_module(validator)


class Args:
    """Minimal argument namespace for validator calls."""

    cohort = "native"
    source_sha = "a" * 40
    run_id = "42"
    iterations = 3
    aggregated_file = None


class ValidateBenchmarkArtifactsTests(unittest.TestCase):
    """Validate strict raw-artifact acceptance and rejection paths."""

    def setUp(self) -> None:
        """Create a complete native cohort artifact tree."""
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.args = Args()
        self.args.artifacts_dir = self.root / "artifacts"
        self.args.cohort_manifest = self.root / "cohort.json"
        self.args.fixtures_root = self.root / "fixtures"
        self.args.artifacts_dir.mkdir()
        self.args.fixtures_root.mkdir()
        self.contract = validator.CONTRACTS["native"]
        self.write_manifest_and_fixtures()
        self.write_artifacts()

    def tearDown(self) -> None:
        """Remove the temporary artifact tree."""
        self.temp.cleanup()

    def write_json(self, path: Path, value: object) -> None:
        """Write one JSON fixture."""
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding="utf-8")

    def write_manifest_and_fixtures(self) -> None:
        """Write the cohort and referenced fixture descriptors."""
        self.write_json(
            self.args.cohort_manifest,
            {
                "schema_version": 1,
                "name": self.contract.manifest_name,
                "batch_size": self.contract.batch_size,
                "fixtures": list(self.contract.fixtures),
            },
        )
        for fixture, document_stem in zip(self.contract.fixtures, self.contract.document_stems, strict=True):
            self.write_json(
                self.args.fixtures_root / fixture,
                {"document": f"../../../test_documents/{document_stem}.pdf"},
            )

    def provenance(self, entry: validator.MatrixEntry) -> dict[str, object]:
        """Build valid provenance for one matrix entry."""
        batch = entry.mode == "batch"
        return {
            "schema_version": 2,
            "repository": {"commit": self.args.source_sha, "dirty": False},
            "corpus": {
                "cohort": self.contract.manifest_name,
                "cohort_manifest_blake3": self.contract.manifest_blake3,
                "ordered_fixtures": [
                    {
                        "fixture": fixture,
                        "fixture_blake3": "b" * 64,
                        "document_blake3": "c" * 64,
                        "document_bytes": 100,
                    }
                    for fixture in self.contract.fixtures
                ],
            },
            "frameworks": [
                {
                    "name": entry.framework,
                    "eligible_documents": len(self.contract.fixtures),
                    "batch_partitions": len(self.contract.fixtures) // self.contract.batch_size if batch else None,
                }
            ],
            "timing": {
                "mode": "Batch" if batch else "SingleFile",
                "benchmark_iterations": self.args.iterations,
                "output_format": entry.output_format,
            },
            "fixed_batch_size": self.contract.batch_size if batch else None,
        }

    def results(self, entry: validator.MatrixEntry) -> list[dict[str, object]]:
        """Build ordered successful results for one matrix entry."""
        return [
            {
                "framework": entry.framework,
                "output_format": entry.output_format,
                "file_path": f"/workspace/test_documents/{Path(fixture).stem}.pdf",
                "success": True,
                "error_kind": "none",
                "error_message": None,
                "ocr_status": "used" if self.args.cohort == "ocr" else "not_used",
                "iterations": [{"iteration": index} for index in range(self.args.iterations)],
            }
            for fixture in self.contract.document_stems
        ]

    def write_artifacts(self) -> None:
        """Write every expected native matrix artifact."""
        for entry in self.contract.matrix:
            artifact = self.args.artifacts_dir / f"{entry.artifact}-{self.args.run_id}" / "run"
            self.write_json(artifact / "provenance.json", self.provenance(entry))
            self.write_json(artifact / "results.json", self.results(entry))

    def select_contract(self, name: str) -> None:
        """Rebuild the fixture tree for another cohort contract."""
        shutil.rmtree(self.args.artifacts_dir)
        shutil.rmtree(self.args.fixtures_root)
        self.args.artifacts_dir.mkdir()
        self.args.fixtures_root.mkdir()
        self.args.cohort = name
        self.contract = validator.CONTRACTS[name]
        self.write_manifest_and_fixtures()
        self.write_artifacts()

    def aggregate(self) -> dict[str, object]:
        """Build an exact aggregate for the selected cohort."""
        expected_ocr = self.args.cohort == "ocr"
        return {
            "schema_version": "2.6.0",
            "by_framework_mode": {
                entry.aggregate_key: {
                    "by_file_type": {
                        "pdf": {
                            "no_ocr": None
                            if expected_ocr
                            else {
                                "total_sample_count": len(self.contract.fixtures),
                                "framework_errors": 0,
                                "harness_errors": 0,
                                "config_setup_errors": 0,
                                "timeouts": 0,
                                "empty_content": 0,
                            },
                            "with_ocr": {
                                "total_sample_count": len(self.contract.fixtures),
                                "framework_errors": 0,
                                "harness_errors": 0,
                                "config_setup_errors": 0,
                                "timeouts": 0,
                                "empty_content": 0,
                            }
                            if expected_ocr
                            else None,
                        }
                    }
                }
                for entry in self.contract.matrix
            },
            "per_fixture_results": [
                {
                    "framework": entry.framework,
                    "output_format": entry.output_format,
                    "execution_mode": "single" if entry.mode == "single-file" else "batch",
                    "fixture_id": fixture,
                    "ocr": expected_ocr,
                    "success": True,
                }
                for entry in self.contract.matrix
                for fixture in self.contract.document_stems
            ],
        }

    def test_accepts_exact_native_contract(self) -> None:
        """The full exact native contract is accepted."""
        validator.validate_artifacts(self.args, self.contract)

    def test_accepts_exact_ocr_contract(self) -> None:
        """The full exact OCR contract is accepted."""
        self.select_contract("ocr")
        validator.validate_artifacts(self.args, self.contract)

    def test_rejects_unexpected_artifact(self) -> None:
        """Unexpected artifact names fail closed."""
        (self.args.artifacts_dir / "benchmarks-surprise-42").mkdir()
        with self.assertRaisesRegex(validator.ContractError, "unexpected"):
            validator.validate_artifacts(self.args, self.contract)

    def test_rejects_source_sha_mismatch(self) -> None:
        """A source revision mismatch fails closed."""
        entry = self.contract.matrix[0]
        path = self.args.artifacts_dir / f"{entry.artifact}-{self.args.run_id}" / "run/provenance.json"
        provenance = json.loads(path.read_text(encoding="utf-8"))
        provenance["repository"]["commit"] = "d" * 40
        self.write_json(path, provenance)
        with self.assertRaisesRegex(validator.ContractError, "source SHA mismatch"):
            validator.validate_artifacts(self.args, self.contract)

    def test_rejects_timeout_result(self) -> None:
        """Timeout rows fail the zero-error release contract."""
        entry = self.contract.matrix[0]
        path = self.args.artifacts_dir / f"{entry.artifact}-{self.args.run_id}" / "run/results.json"
        results = json.loads(path.read_text(encoding="utf-8"))
        results[0]["success"] = False
        results[0]["error_kind"] = "timeout"
        self.write_json(path, results)
        with self.assertRaisesRegex(validator.ContractError, "failed"):
            validator.validate_artifacts(self.args, self.contract)

    def test_rejects_duplicate_fixture_result(self) -> None:
        """Duplicate or reordered fixture rows fail closed."""
        entry = self.contract.matrix[0]
        path = self.args.artifacts_dir / f"{entry.artifact}-{self.args.run_id}" / "run/results.json"
        results = json.loads(path.read_text(encoding="utf-8"))
        results[1]["file_path"] = results[0]["file_path"]
        self.write_json(path, results)
        with self.assertRaisesRegex(validator.ContractError, "order/content mismatch"):
            validator.validate_artifacts(self.args, self.contract)

    def test_rejects_malformed_provenance(self) -> None:
        """Malformed provenance JSON fails closed."""
        entry = self.contract.matrix[0]
        path = self.args.artifacts_dir / f"{entry.artifact}-{self.args.run_id}" / "run/provenance.json"
        path.write_text("{", encoding="utf-8")
        with self.assertRaisesRegex(validator.ContractError, "malformed"):
            validator.validate_artifacts(self.args, self.contract)

    def test_accepts_exact_aggregate_contract(self) -> None:
        """The consolidated cohort must retain every exact capability key."""
        path = self.root / "aggregated.json"
        self.write_json(path, self.aggregate())
        self.args.aggregated_file = path
        validator.validate_aggregate(self.args, self.contract)

    def test_accepts_exact_ocr_aggregate_contract(self) -> None:
        """The OCR aggregate retains the forced-OCR bucket and exact keys."""
        self.select_contract("ocr")
        path = self.root / "aggregated-ocr.json"
        self.write_json(path, self.aggregate())
        self.args.aggregated_file = path
        validator.validate_aggregate(self.args, self.contract)

    def test_rejects_unexpected_aggregate_key(self) -> None:
        """An unexpected consolidated key fails closed."""
        path = self.root / "aggregated.json"
        self.write_json(
            path,
            {
                "schema_version": "2.6.0",
                "by_framework_mode": {"surprise:markdown:single": {}},
                "per_fixture_results": [],
            },
        )
        self.args.aggregated_file = path
        with self.assertRaisesRegex(validator.ContractError, "unexpected"):
            validator.validate_aggregate(self.args, self.contract)

    def test_contract_key_counts_are_exact(self) -> None:
        """Capability matrices contain the documented unique keys."""
        self.assertEqual(len(validator.CONTRACTS["native"].matrix), 23)
        self.assertEqual(len(validator.CONTRACTS["ocr"].matrix), 18)
        for contract in validator.CONTRACTS.values():
            self.assertEqual(len({entry.artifact for entry in contract.matrix}), len(contract.matrix))
            self.assertEqual(len({entry.aggregate_key for entry in contract.matrix}), len(contract.matrix))


if __name__ == "__main__":
    unittest.main()
