#!/usr/bin/env bash
# Loads a built musl xberg_nif shared library directly (bypassing
# RustlerPrecompiled's release-download / force_build paths -- see
# scripts/ci/smoke_test_elixir_nif.exs for why) inside a genuine Alpine (musl
# libc) container, and runs a real extraction against a known fixture,
# asserting the exact expected text comes back. Same rationale as
# xberg-io/xberg#490 for the Python wheel: a file-exists check on the .so
# proves nothing about whether it actually loads and runs.
#
# Deliberately does NOT run on the GitHub Actions host directly: the host is
# glibc (ubuntu-latest / ubuntu-24.04-arm), and the entire point of the gate
# is proving the library works on the musl libc family it was cross-compiled
# for.
set -euo pipefail

log() { echo "smoke-test-musl-elixir-nif: $*" >&2; }
die() { log "$*"; exit 1; }

NATIVE_DIR="${1:?usage: $0 <dir-containing-libxberg_nif.so-and-vendored-.so-closure>}"
[ -d "$NATIVE_DIR" ] || die "native asset directory not found: $NATIVE_DIR"
# `docker -v` rejects a relative source as a volume NAME ("includes invalid
# characters for a local volume name"), so this must be absolute before the
# mount below. Matches smoke-test-musl-python-wheel.sh, which already does it. ~keep
NATIVE_DIR="$(cd "$NATIVE_DIR" && pwd)"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="$REPO_ROOT/scripts/ci/fixtures/musl-python-smoke.pdf"
SMOKE_SCRIPT="$REPO_ROOT/scripts/ci/smoke_test_elixir_nif.exs"
EXPECTED_TEXT="XBERG MUSL SMOKE 490"

[ -f "$FIXTURE" ] || die "smoke-test fixture missing: $FIXTURE"
[ -f "$SMOKE_SCRIPT" ] || die "smoke-test script missing: $SMOKE_SCRIPT"
find "$NATIVE_DIR" -maxdepth 1 -name 'libxberg_nif.so' | grep -q . || die "no libxberg_nif.so found under $NATIVE_DIR"

# alpine:3.21 matches docker/Dockerfile.musl-rustler's build image, so this is
# the same musl libc family the NIF was cross-compiled for. The whole
# directory is mounted (not just libxberg_nif.so) so the vendored ONNX
# Runtime / image-codec closure Dockerfile.musl-rustler now bundles beside it
# is present for the musl loader to resolve via libxberg_nif.so's $ORIGIN
# RUNPATH -- mounting only the .so file left every bundled dependency behind
# and made a missing-transitive-library failure look like a NIF load failure
# with no name attached (xberg #1280).
docker run --rm \
  -v "$NATIVE_DIR:/native:ro" \
  -v "$FIXTURE:/fixture.pdf:ro" \
  -v "$SMOKE_SCRIPT:/smoke/smoke_test_elixir_nif.exs:ro" \
  -e XBERG_NIF_PATH="/native/libxberg_nif" \
  -e EXPECTED_TEXT="$EXPECTED_TEXT" \
  alpine:3.21 sh -euc '
    apk add --no-cache elixir >/dev/null
    elixir /smoke/smoke_test_elixir_nif.exs /fixture.pdf "${EXPECTED_TEXT}"
  '
