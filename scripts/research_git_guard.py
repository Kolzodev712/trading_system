#!/usr/bin/env python3
"""Refuse generated research datasets, ledgers, and reports before they enter Git.

Existing tracked files are left alone. A later deletion does not remove an
earlier Git object, so this check must run before the first add/commit.
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

GENERATED_PREFIXES = (
    "data/",
)
GENERATED_NAME_MARKERS = (
    "/backtest/",
    "/decisions-",
    "/events-",
    "/coverage_gate.json",
    "/trade-60.json",
    "/trade-1m.json",
)
ALLOWED_NEW = (
    "research/candidates/",
    "research/datasets/",
    "research/schema/",
    "research/tests/",
    "research/README.md",
    "research/INDEX.md",
    "scripts/research_artifacts.py",
    "scripts/research_git_guard.py",
)
MAX_NEW_DATA_BYTES = 256 * 1024
MAX_NEW_ANY_BYTES = 1024 * 1024


def git(root: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(root), *args],
        text=True,
        capture_output=True,
        check=check,
    )


def tracked(root: Path, rel: str) -> bool:
    result = git(root, "ls-files", "--error-unmatch", rel, check=False)
    return result.returncode == 0


def staged_and_untracked(root: Path) -> list[str]:
    names = []
    staged = git(root, "diff", "--cached", "--name-only", "--diff-filter=ACMR").stdout
    untracked = git(root, "ls-files", "--others", "--exclude-standard").stdout
    for line in (staged + untracked).splitlines():
        rel = line.strip()
        if rel:
            names.append(rel)
    return sorted(set(names))


def is_generated(rel: str) -> bool:
    if not rel.startswith(GENERATED_PREFIXES):
        return False
    return any(marker in f"/{rel}" or rel.endswith(marker.lstrip("/")) for marker in GENERATED_NAME_MARKERS) or "/backtest/" in rel


def allowed_new(rel: str) -> bool:
    return any(rel == prefix or rel.startswith(prefix) for prefix in ALLOWED_NEW)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".")
    args = parser.parse_args()
    root = Path(args.repo).resolve()
    probe = git(root, "rev-parse", check=False)
    if probe.returncode != 0:
        print("error: not a git repository", file=sys.stderr)
        return 2

    problems = []
    for rel in staged_and_untracked(root):
        path = root / rel
        if tracked(root, rel):
            continue
        if not path.is_file():
            continue
        size = path.stat().st_size
        if is_generated(rel):
            problems.append(f"{rel}: generated research output must stay out of Git ({size} bytes)")
            continue
        if rel.startswith("data/") and size > MAX_NEW_DATA_BYTES and not allowed_new(rel):
            problems.append(
                f"{rel}: new data file {size} bytes exceeds {MAX_NEW_DATA_BYTES}"
            )
            continue
        if size > MAX_NEW_ANY_BYTES and not rel.startswith("research/tests/"):
            problems.append(f"{rel}: new file {size} bytes exceeds {MAX_NEW_ANY_BYTES}")
    if problems:
        print("research git guard failed:", file=sys.stderr)
        for item in problems:
            print(f"  {item}", file=sys.stderr)
        return 1
    print("research git guard: ok")
    return 0


if __name__ == "__main__":
    # Use the caller's git directory; paths from git are repo-relative.
    sys.exit(main())
