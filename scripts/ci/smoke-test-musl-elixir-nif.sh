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

NIF_SO="${1:?usage: $0 <path-to-libxberg_nif.so>}"
[ -f "$NIF_SO" ] || die "NIF shared library not found: $NIF_SO"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="$REPO_ROOT/scripts/ci/fixtures/musl-python-smoke.pdf"
SMOKE_SCRIPT="$REPO_ROOT/scripts/ci/smoke_test_elixir_nif.exs"
EXPECTED_TEXT="XBERG MUSL SMOKE 490"

[ -f "$FIXTURE" ] || die "smoke-test fixture missing: $FIXTURE"
[ -f "$SMOKE_SCRIPT" ] || die "smoke-test script missing: $SMOKE_SCRIPT"

# alpine:3.21 matches docker/Dockerfile.musl-rustler's build image, so this is
# the same musl libc family the NIF was cross-compiled for.
docker run --rm \
  -v "$NIF_SO:/native/libxberg_nif.so:ro" \
  -v "$FIXTURE:/fixture.pdf:ro" \
  -v "$SMOKE_SCRIPT:/smoke/smoke_test_elixir_nif.exs:ro" \
  -e XBERG_NIF_PATH="/native/libxberg_nif" \
  -e EXPECTED_TEXT="$EXPECTED_TEXT" \
  alpine:3.21 sh -euc '
    apk add --no-cache elixir >/dev/null
    elixir /smoke/smoke_test_elixir_nif.exs /fixture.pdf "${EXPECTED_TEXT}"
  '
