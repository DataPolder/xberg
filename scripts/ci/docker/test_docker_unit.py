"""Unit tests for Docker CI helpers that do not require a Docker daemon."""

from __future__ import annotations

import unittest
from pathlib import Path

from scripts.ci.docker.test_docker import CLI_IMAGE_LIMIT_BYTES, cli_image_size_is_allowed, format_image_size


class CliImageSizeTests(unittest.TestCase):
    def test_accepts_last_byte_below_limit(self) -> None:
        assert cli_image_size_is_allowed(CLI_IMAGE_LIMIT_BYTES - 1) is True

    def test_rejects_exact_exclusive_limit(self) -> None:
        assert cli_image_size_is_allowed(CLI_IMAGE_LIMIT_BYTES) is False

    def test_rejects_empty_or_unparseable_size(self) -> None:
        assert cli_image_size_is_allowed(0) is False

    def test_reports_exact_bytes_and_mebibytes(self) -> None:
        assert format_image_size(CLI_IMAGE_LIMIT_BYTES) == "250.000000 MiB (262144000 bytes)"


class CliDockerfileProfileTests(unittest.TestCase):
    def test_preserves_and_uses_release_musl_profile(self) -> None:
        dockerfile = Path("docker/Dockerfile.cli").read_text(encoding="utf-8")

        assert "profile\\.release\\.package\\.xberg-wasm\\]$/,/^$/d" in dockerfile
        assert "cargo build --profile release-musl --package xberg-cli --features all" in dockerfile
        assert "cp target/release-musl/xberg /build/xberg" in dockerfile


if __name__ == "__main__":
    unittest.main()
