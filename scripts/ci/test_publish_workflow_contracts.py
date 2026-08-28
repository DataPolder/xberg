import re
from pathlib import Path

WORKFLOW = Path(__file__).parents[2] / ".github" / "workflows" / "publish.yaml"


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
    }
    workflow = WORKFLOW.read_text()
    for job, action in jobs_and_actions.items():
        block = job_block(workflow, job)
        setup = block.index('echo "LZMA_API_STATIC=1" >> "$GITHUB_ENV"')
        build = block.index(action)
        assert setup < build, f"{job} configures static liblzma after its build"
        setup_block = block[block.rfind("\n      - ", 0, setup) : setup]
        assert "contains(matrix.target, '-linux-gnu')" in setup_block, job
