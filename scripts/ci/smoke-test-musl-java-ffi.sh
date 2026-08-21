#!/usr/bin/env bash
# Installs a built musl xberg-ffi shared library into a genuine Alpine (musl
# libc) container, compiles the io.xberg Java sources against it with a real
# JDK, and runs a real extraction against a known fixture, asserting the
# exact expected text comes back. See scripts/ci/fixtures/java-smoke/Smoke.java
# for why loading the library is not sufficient on its own here (same
# rationale as xberg-io/xberg#490 for the Python wheel).
#
# Deliberately does NOT run on the GitHub Actions host directly: the host is
# glibc (ubuntu-latest / ubuntu-24.04-arm), and the entire point of the gate
# is proving the library works on the musl libc family it was cross-compiled
# for.
set -euo pipefail

log() { echo "smoke-test-musl-java-ffi: $*" >&2; }
die() { log "$*"; exit 1; }

BUILD_OUTPUT_DIR="${1:?usage: $0 <build-output-dir-containing-libxberg_ffi.so>}"
[ -d "$BUILD_OUTPUT_DIR" ] || die "build-output directory not found: $BUILD_OUTPUT_DIR"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="$REPO_ROOT/scripts/ci/fixtures/musl-python-smoke.pdf"
JAVA_SRC_DIR="$REPO_ROOT/packages/java/io/xberg"
SMOKE_JAVA="$REPO_ROOT/scripts/ci/fixtures/java-smoke/Smoke.java"
POM="$REPO_ROOT/packages/java/pom.xml"
EXPECTED_TEXT="XBERG MUSL SMOKE 490"

[ -f "$FIXTURE" ] || die "smoke-test fixture missing: $FIXTURE"
[ -d "$JAVA_SRC_DIR" ] || die "io.xberg Java sources missing: $JAVA_SRC_DIR"
[ -f "$SMOKE_JAVA" ] || die "smoke-test program missing: $SMOKE_JAVA"
[ -f "$POM" ] || die "packages/java/pom.xml missing: $POM"

LIB_FILE="$(find "$BUILD_OUTPUT_DIR" -maxdepth 1 -name 'libxberg_ffi.so' | head -n1)"
[ -n "$LIB_FILE" ] || die "no libxberg_ffi.so found under $BUILD_OUTPUT_DIR"

# alpine:3.21 matches docker/Dockerfile.musl-ffi's build image, so this is the
# same musl libc family the library was cross-compiled for. maven resolves
# io.xberg's compile-scope dependencies (jackson, jspecify) from Maven
# Central inside the container -- normal network access for a CI runner, not
# for this script's author.
docker run --rm \
  -v "$REPO_ROOT/packages/java:/repo/packages/java:ro" \
  -v "$SMOKE_JAVA:/smoke/Smoke.java:ro" \
  -v "$LIB_FILE:/native/libxberg_ffi.so:ro" \
  -v "$FIXTURE:/fixture.pdf:ro" \
  -e EXPECTED_TEXT="$EXPECTED_TEXT" \
  -e XBERG_FFI_LIB_PATH="/native/libxberg_ffi.so" \
  alpine:3.21 sh -euc '
    apk add --no-cache openjdk25-jdk maven \
      --repository=https://dl-cdn.alpinelinux.org/alpine/edge/community \
      --repository=https://dl-cdn.alpinelinux.org/alpine/edge/main
    mvn -q -f /repo/packages/java/pom.xml -DincludeScope=compile \
      dependency:build-classpath -Dmdep.outputFile=/tmp/cp.txt
    CP="$(cat /tmp/cp.txt)"
    mkdir -p /tmp/out
    javac --release 25 --enable-preview -cp "$CP" -d /tmp/out \
      /repo/packages/java/io/xberg/*.java /smoke/Smoke.java
    java --enable-preview --enable-native-access=ALL-UNNAMED \
      -cp "/tmp/out:$CP" Smoke /fixture.pdf "${EXPECTED_TEXT}"
  '
