#!/usr/bin/env python3
"""Research git-size guard tests."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "research_git_guard.py"


class GitGuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="research-guard-"))
        subprocess.check_call(["git", "init"], cwd=self.tmp, stdout=subprocess.DEVNULL)
        subprocess.check_call(["git", "config", "user.email", "test@example.com"], cwd=self.tmp)
        subprocess.check_call(["git", "config", "user.name", "Test"], cwd=self.tmp)
        (self.tmp / "keep.txt").write_text("ok\n")
        subprocess.check_call(["git", "add", "keep.txt"], cwd=self.tmp)
        subprocess.check_call(["git", "commit", "-m", "base"], cwd=self.tmp, stdout=subprocess.DEVNULL)

    def run_guard(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--repo", str(self.tmp)],
            text=True,
            capture_output=True,
        )

    def test_allows_small_record_files(self):
        dest = self.tmp / "research" / "candidates" / "example" / "SPEC.md"
        dest.parent.mkdir(parents=True)
        dest.write_text("# spec\n")
        result = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_blocks_new_generated_backtest(self):
        dest = self.tmp / "data" / "example" / "backtest" / "summary.json"
        dest.parent.mkdir(parents=True)
        dest.write_text("{}\n")
        result = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("generated research output", result.stderr)

    def test_blocks_oversized_new_data_file(self):
        dest = self.tmp / "data" / "example" / "notes.bin"
        dest.parent.mkdir(parents=True)
        dest.write_bytes(b"x" * (256 * 1024 + 1))
        result = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exceeds", result.stderr)

    def test_existing_tracked_large_file_is_left_alone(self):
        dest = self.tmp / "data" / "legacy" / "trade-1m.json"
        dest.parent.mkdir(parents=True)
        dest.write_bytes(b"y" * (300 * 1024))
        subprocess.check_call(["git", "add", "-f", "data/legacy/trade-1m.json"], cwd=self.tmp)
        subprocess.check_call(["git", "commit", "-m", "legacy"], cwd=self.tmp, stdout=subprocess.DEVNULL)
        result = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
