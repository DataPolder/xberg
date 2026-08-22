#!/usr/bin/env python3
"""Guard: every path a Dockerfile COPYs must survive `.dockerignore`.

`.dockerignore` ignores everything (`*`) and re-includes a hand-written allowlist. A
Dockerfile that COPYs a path missing from that allowlist does not warn or copy nothing --
buildx fails the whole build with `"/crates/<name>": not found`.

This has bitten three times. `f3043a3709` added crates/xberg-libwpd after the fact,
`6b203eae18` added crates/ttf-parser-compat, and `7b764bedba` made xberg-pdfium-render a
workspace member and added a COPY line to all ten Dockerfiles without touching the
allowlist -- which broke every published image build for a day, unnoticed, because
`ci-docker.yaml` only triggers on `docker/**` and two specific Cargo.toml paths, and that
commit touched neither.

The reverse direction matters too: `crates/ttf-parser-compat/` sat in the allowlist long
after the crate was deleted. A stale entry is harmless to the build but is exactly the
noise that makes a real omission hard to see.

Exit codes: 0 = context is consistent, 1 = a COPY would fail or an entry is dead, 2 = bad input.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

DOCKERIGNORE = Path(".dockerignore")
DOCKER_DIR = Path("docker")

# `COPY <src> <dst>` where src is a repo path we care about. Flags (--from=, --chown=) are
# skipped: a `COPY --from=<stage>` reads from an earlier stage, not the build context, so it
# is not subject to .dockerignore at all.
_COPY = re.compile(r"^\s*COPY\s+(?!--)(?P<srcs>.+?)\s+\S+\s*$", re.MULTILINE)
_ALLOW = re.compile(r"^!(?P<path>\S+?)/?\s*$", re.MULTILINE)


def _fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)


def copied_paths(docker_dir: Path) -> dict[str, list[str]]:
    """Map each context path a Dockerfile COPYs to the Dockerfiles that copy it."""
    found: dict[str, list[str]] = {}
    for dockerfile in sorted(docker_dir.glob("Dockerfile*")):
        text = dockerfile.read_text(encoding="utf-8", errors="replace")
        for match in _COPY.finditer(text):
            for src in match.group("srcs").split():
                # Only directory-ish repo paths are allowlisted individually; bare files at
                # the root (Cargo.toml, Cargo.lock) have their own entries and `.` is the
                # whole context.
                if src in {".", "./"} or not src.endswith("/"):
                    continue
                found.setdefault(src.rstrip("/"), []).append(dockerfile.name)
    return found


def allowlisted(dockerignore: Path) -> set[str]:
    return {m.group("path") for m in _ALLOW.finditer(dockerignore.read_text(encoding="utf-8"))}


def check(dockerignore: Path, docker_dir: Path) -> int:
    copied = copied_paths(docker_dir)
    allowed = allowlisted(dockerignore)
    if not copied:
        _fail(f"{docker_dir}: found no COPY directives -- this check would pass vacuously")
        return 2

    failed = False

    missing = {path: files for path, files in copied.items() if path not in allowed}
    if missing:
        for path, files in sorted(missing.items()):
            _fail(
                f"{path} is COPYd by {', '.join(sorted(set(files)))} but is not allowlisted in "
                f"{dockerignore}. buildx will fail with '\"/{path}\": not found'. "
                f"Add `!{path}/`."
            )
        failed = True

    dead = sorted(entry for entry in allowed if entry.startswith("crates/") and not Path(entry).is_dir())
    if dead:
        _fail(
            f"{dockerignore} allowlists {', '.join(dead)}, which no longer exist(s) on disk. "
            f"Harmless to the build, but stale entries are what hide a real omission."
        )
        failed = True

    if failed:
        return 1

    dockerfiles = {name for files in copied.values() for name in files}
    print(
        f"OK: all {len(copied)} COPYd context path(s) across {len(dockerfiles)} "
        f"Dockerfile(s) are allowlisted, no dead entries"
    )
    return 0


def main() -> int:
    if not DOCKERIGNORE.is_file() or not DOCKER_DIR.is_dir():
        _fail("run from the repository root (.dockerignore and docker/ must exist)")
        return 2
    return check(DOCKERIGNORE, DOCKER_DIR)


if __name__ == "__main__":
    sys.exit(main())
