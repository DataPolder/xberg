import re
from pathlib import Path

WORKFLOW = Path(__file__).parents[2] / ".github" / "workflows" / "publish.yaml"
CI_WORKFLOW = Path(__file__).parents[2] / ".github" / "workflows" / "ci-lint.yaml"


def job_block(workflow: str, job: str) -> str:
    match = re.search(rf"(?ms)^  {re.escape(job)}:\n(.*?)(?=^  [a-zA-Z0-9_-]+:\n|\Z)", workflow)
    if match is None:
        raise AssertionError(f"publish workflow has no {job!r} job")
    return match.group(1)


def test_swift_dry_run_checks_run_artifact_without_release_mutation() -> None:
    block = job_block(WORKFLOW.read_text(), "update-swift-package-manifest")
    assert "name: swift-artifactbundle" in block
    assert "needs.prepare.outputs.dry_run == 'true'" in block
    assert 'if [ "$DRY_RUN" != "true" ]; then' in block
    assert "gh release download" in block

    for step in (
        "Update Package.swift with version and checksum",
        "Commit and push Package.swift",
    ):
        mutation = block.index(f"- name: {step}")
        next_step = block.find("\n      - ", mutation + 1)
        step_block = block[mutation : next_step if next_step >= 0 else None]
        assert "needs.prepare.outputs.dry_run != 'true'" in step_block


def test_glibc_ffi_jobs_build_lzma_statically() -> None:
    jobs_and_actions = {
        "go-ffi-libraries": "xberg-io/actions/build-go-ffi@v1",
        "c-ffi-libraries": "xberg-io/actions/build-rust-ffi@v1",
        "java-natives": "xberg-io/actions/build-java-natives@v1",
        "csharp-natives": "xberg-io/actions/build-csharp-natives@v1",
        "elixir-natives": "xberg-io/actions/build-elixir-natives@v1",
        "dart-server-natives": "- name: Build Dart server native",
    }
    workflow = WORKFLOW.read_text()
    for job, action in jobs_and_actions.items():
        block = job_block(workflow, job)
        setup = block.index('echo "LZMA_API_STATIC=1" >> "$GITHUB_ENV"')
        build = block.index(action)
        assert setup < build, f"{job} configures static liblzma after its build"
        setup_block = block[block.rfind("\n      - ", 0, setup) : setup]
        assert "contains(matrix.target, '-linux-gnu')" in setup_block, job


def test_glibc_native_closures_are_strictly_verified() -> None:
    workflow = WORKFLOW.read_text()
    for job in ("csharp-natives", "dart-server-natives"):
        block = job_block(workflow, job)
        vendor = block.index("scripts/ci/vendor-native-closure.sh")
        verify = block.index("scripts/ci/verify-glibc-floor.sh")
        assert vendor < verify, f"{job} verifies before vendoring its closure"


def test_publish_contracts_run_in_ci() -> None:
    assert "python3 scripts/ci/test_publish_workflow_contracts.py" in CI_WORKFLOW.read_text()


if __name__ == "__main__":
    test_swift_dry_run_checks_run_artifact_without_release_mutation()
    test_glibc_ffi_jobs_build_lzma_statically()
    test_glibc_native_closures_are_strictly_verified()
    test_publish_contracts_run_in_ci()
