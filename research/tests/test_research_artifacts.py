#!/usr/bin/env python3
"""Artifact store and manifest tests. No venue, credentials, or running services."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "research_artifacts.py"
if str(ROOT / "scripts") not in sys.path:
    sys.path.insert(0, str(ROOT / "scripts"))

import research_artifacts as ra  # noqa: E402


def valid_manifest(**overrides):
    data = {
        "schema_version": 1,
        "candidate_id": "ExampleShockV1",
        "candidate_version": "1",
        "status": ["REJECTED", "ARCHIVED"],
        "lifecycle": {
            "reached": "PRIMARY_RESEARCH",
            "preregistered_on_master": False,
            "preregistration_note": "test",
        },
        "experiment_window": {
            "start_utc": "2023-01-21T00:00:00Z",
            "end_exclusive_utc": "2026-01-01T00:00:00Z",
            "semantics": "end_exclusive",
        },
        "spec": {
            "path": "research/candidates/example/SPEC.md",
            "sha256": "sha256:" + "a" * 64,
            "experiment_plan_path": "research/candidates/example/experiment.json",
            "experiment_plan_sha256": "sha256:" + "b" * 64,
        },
        "scored_source": {
            "revision": "8e20b5f53b83101c2dba05e823d9ad69d91ccd9e",
            "dirty": False,
            "command": "research_cli example run",
            "reconstruction": "git bundle inside the archive",
        },
        "datasets": [
            {
                "id": "example_trade_60",
                "sha256": "c" * 64,
                "content_hash": "sha256:" + "d" * 64,
                "content_hash_kind": "research_engine_dataset_hash",
                "role": "hourly_trade",
            }
        ],
        "results": {"completed_trades": 1, "cost_a_net": "-1.25"},
        "cost_assumptions": {"cost_a": "6bps+2bps+10s"},
        "artifact": {"id": "example-v1-primary", "sha256": "e" * 64},
        "gates": {
            "primary": {"state": "FAILED", "note": "cost a negative"},
            "robustness": {"state": "NOT_OPENED", "note": "primary failed"},
            "independent_audit": {"state": "NOT_PERFORMED", "note": "primary failed"},
            "holdout": {
                "state": "UNOPENED",
                "note": "2026 sealed; not validation",
                "used_as_validation": False,
            },
        },
    }
    data.update(overrides)
    return data


class ManifestTests(unittest.TestCase):
    def test_valid_manifest(self):
        ra.validate_manifest(valid_manifest())

    def test_rejects_numeric_result(self):
        bad = valid_manifest()
        bad["results"]["cost_a_net"] = -1.25
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(bad)

    def test_rejects_rejected_without_archived(self):
        bad = valid_manifest()
        bad["status"] = ["REJECTED"]
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(bad)

    def test_rejects_holdout_passed_unopened(self):
        bad = valid_manifest()
        bad["gates"]["holdout"] = {
            "state": "UNOPENED",
            "note": "sealed",
            "used_as_validation": True,
        }
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(bad)

    def test_rejects_unknown_status(self):
        bad = valid_manifest()
        bad["status"] = ["DONE"]
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(bad)

    def test_rejects_inclusive_window_semantics(self):
        bad = valid_manifest()
        bad["experiment_window"]["semantics"] = "inclusive"
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(bad)

    def test_index_tracks_manifest(self):
        text = ra.generate_index([valid_manifest()])
        self.assertIn("ExampleShockV1", text)
        self.assertIn("REJECTED+ARCHIVED", text)

    def test_legacy_unrecorded_provenance_is_valid(self):
        data = valid_manifest()
        data["scored_source"] = {
            "provenance": "legacy_unrecorded",
            "ledger_revision": "2cd8211f89aa543f504c13ef9a84051e350b7d62",
            "contemporaneous_command": (
                "cargo run -p runner --bin donchian_daily_replay -- evaluate"
            ),
            "reconstruction": (
                "Ledgers first tracked at ledger_revision. "
                "The original scoring SHA and dirty state were not recorded."
            ),
        }
        ra.validate_manifest(data)

    def test_legacy_unrecorded_rejects_evidence_commit_as_clean_score(self):
        data = valid_manifest()
        data["scored_source"] = {
            "provenance": "legacy_unrecorded",
            "revision": "2cd8211f89aa543f504c13ef9a84051e350b7d62",
            "dirty": False,
            "command": "cargo run -p research_cli -- donchian-daily evaluate",
            "ledger_revision": "2cd8211f89aa543f504c13ef9a84051e350b7d62",
            "reconstruction": "test",
        }
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(data)

    def test_legacy_unrecorded_rejects_research_cli_as_contemporaneous(self):
        data = valid_manifest()
        data["scored_source"] = {
            "provenance": "legacy_unrecorded",
            "ledger_revision": "2cd8211f89aa543f504c13ef9a84051e350b7d62",
            "contemporaneous_command": "cargo run -p research_cli -- btc-price-ma run",
            "reconstruction": "test",
        }
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(data)

    def test_legacy_unrecorded_rejects_missing_ledger_revision(self):
        data = valid_manifest()
        data["scored_source"] = {
            "provenance": "legacy_unrecorded",
            "contemporaneous_command": (
                "cargo run -p runner --bin btc_price_ma_experiment -- run"
            ),
            "reconstruction": "test",
        }
        with self.assertRaises(ra.ArtifactError):
            ra.validate_manifest(data)

    def test_index_marks_legacy_score_unrecorded(self):
        data = valid_manifest()
        data["candidate_id"] = "LegacyScreenV1"
        data["scored_source"] = {
            "provenance": "legacy_unrecorded",
            "ledger_revision": "2cd8211f89aa543f504c13ef9a84051e350b7d62",
            "contemporaneous_command": (
                "cargo run -p runner --bin donchian_daily_replay -- evaluate"
            ),
            "reconstruction": "ledger commit is not a scoring SHA",
        }
        text = ra.generate_index([data])
        self.assertIn("`unrecorded`", text)
        self.assertNotIn("2cd8211", text)


class StoreTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="research-store-"))
        self.root = self.tmp / "store"
        self.repo = self.tmp / "repo"
        self.repo.mkdir()
        subprocess.check_call(["git", "init"], cwd=self.repo, stdout=subprocess.DEVNULL)
        subprocess.check_call(["git", "config", "user.email", "test@example.com"], cwd=self.repo)
        subprocess.check_call(["git", "config", "user.name", "Test"], cwd=self.repo)
        (self.repo / "keep.txt").write_text("base\n")
        subprocess.check_call(["git", "add", "keep.txt"], cwd=self.repo)
        subprocess.check_call(["git", "commit", "-m", "base"], cwd=self.repo, stdout=subprocess.DEVNULL)
        self.base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.repo, text=True).strip()
        (self.repo / "spec.md").write_text("# spec\n")
        (self.repo / "run.json").write_text('{"ok":true}\n')
        data = self.repo / "data" / "set.json"
        data.parent.mkdir()
        data.write_text('{"rows":[1]}\n')
        subprocess.check_call(["git", "add", "spec.md", "run.json", "data/set.json"], cwd=self.repo)
        subprocess.check_call(["git", "commit", "-m", "scored"], cwd=self.repo, stdout=subprocess.DEVNULL)
        self.tip = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.repo, text=True).strip()
        self.dataset_sha = ra.sha256_file(data)
        self.allowlist = {
            "artifact_id": "example-v1-primary",
            "files": [
                {"path": "spec.md", "role": "spec"},
                {"path": "run.json", "role": "results"},
            ],
            "datasets": [
                {
                    "id": "example_set",
                    "path": "data/set.json",
                    "sha256": self.dataset_sha,
                    "content_hash": "sha256:" + "d" * 64,
                    "content_hash_kind": "research_engine_dataset_hash",
                    "role": "fixture",
                }
            ],
            "source": {
                "base": self.base,
                "tip": self.tip,
                "command": "true",
                "reconstruction": "git clone --mirror source.bundle",
            },
            "commands": {"score": "true"},
            "pin": {"revision": self.tip},
        }
        self.allowlist_path = self.tmp / "allowlist.json"
        self.allowlist_path.write_text(json.dumps(self.allowlist, indent=2))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_tool(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["RESEARCH_ARTIFACT_ROOT"] = str(self.root)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            text=True,
            capture_output=True,
            check=False,
            env=env,
        )
        if check and result.returncode != 0:
            raise AssertionError(result.stderr or result.stdout)
        return result

    def test_pack_verify_restore_and_idempotent_retry(self):
        first = self.run_tool("pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo))
        digest = [line.split("=", 1)[1] for line in first.stdout.splitlines() if line.startswith("sha256=")][0]
        second = self.run_tool("pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo))
        self.assertIn(digest, second.stdout)
        self.run_tool("verify", "example-v1-primary")
        dest = self.tmp / "restore"
        self.run_tool("restore", "example-v1-primary", "--dest", str(dest))
        self.assertTrue((dest / "spec.md").is_file())
        self.assertTrue((dest / "data" / "set.json").is_file())
        self.assertEqual(ra.sha256_file(dest / "data" / "set.json"), self.dataset_sha)
        self.assertFalse((dest / "SHA256").exists())

    def test_conflict_when_id_points_at_different_bytes(self):
        self.run_tool("pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo))
        (self.repo / "run.json").write_text('{"ok":false}\n')
        subprocess.check_call(["git", "add", "run.json"], cwd=self.repo)
        subprocess.check_call(["git", "commit", "-m", "changed"], cwd=self.repo, stdout=subprocess.DEVNULL)
        tip = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.repo, text=True).strip()
        self.allowlist["source"]["tip"] = tip
        self.allowlist_path.write_text(json.dumps(self.allowlist))
        result = self.run_tool(
            "pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo), check=False
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already points to", result.stderr)

    def test_missing_and_wrong_dataset_hash(self):
        self.run_tool("pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo))
        dataset = next((self.root / "datasets" / "sha256").rglob("*"))
        while dataset.is_dir():
            dataset = next(dataset.iterdir())
        dataset.unlink()
        result = self.run_tool("verify", "example-v1-primary", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(
            "absent" in result.stderr or "hash" in result.stderr,
            result.stderr,
        )

        self.run_tool("pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo))
        dataset = next(p for p in (self.root / "datasets" / "sha256").rglob("*") if p.is_file())
        dataset.write_bytes(b"wrong")
        result = self.run_tool("verify", "example-v1-primary", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("hash", result.stderr)

    def test_interrupted_publication_does_not_publish_partial(self):
        root = ra.artifact_root(str(self.root))
        dest = ra.object_path(root, "f" * 64, "objects")
        dest.parent.mkdir(parents=True, exist_ok=True)
        partial = dest.with_name(f".{dest.name}.99999.tmp")
        partial.write_bytes(b"partial")
        self.assertFalse(dest.exists())
        source = self.tmp / "complete.bin"
        source.write_bytes(b"complete-bytes")
        digest = ra.sha256_file(source)
        ra.publish_bytes(ra.object_path(root, digest, "objects"), source, digest)
        self.assertTrue(ra.object_path(root, digest, "objects").is_file())
        self.assertFalse(dest.exists())

    def test_malicious_archive_paths_rejected(self):
        dest = self.tmp / "escape"
        dest.mkdir()
        archive = self.tmp / "evil.tar"
        with tarfile.open(archive, "w") as handle:
            info = tarfile.TarInfo("../outside.txt")
            payload = b"nope"
            info.size = len(payload)
            handle.addfile(info, fileobj=__import__("io").BytesIO(payload))
        with self.assertRaises(ra.ArtifactError):
            ra.extract_archive(archive, dest)
        self.assertFalse((self.tmp / "outside.txt").exists())

    def test_secret_file_is_refused(self):
        secret = self.repo / ".env"
        secret.write_text("apiSecret=nope\n")
        self.allowlist["files"].append({"path": ".env", "role": "secret"})
        self.allowlist_path.write_text(json.dumps(self.allowlist))
        result = self.run_tool(
            "pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo), check=False
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("secret", result.stderr)

    def test_same_disk_is_not_a_backup(self):
        self.run_tool("pack", "--allowlist", str(self.allowlist_path), "--repo", str(self.repo))
        backup = self.tmp / "backup-same-disk"
        result = self.run_tool(
            "backup",
            "example-v1-primary",
            "--backup-root",
            str(backup),
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("same physical device", result.stderr)


if __name__ == "__main__":
    unittest.main()
