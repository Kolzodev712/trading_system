#!/usr/bin/env python3
"""Local research artifact store: pack, verify, restore, index.

Large datasets are stored once by SHA-256. Git keeps logical IDs and hashes,
never machine-specific paths. The outer archive SHA is recorded in Git, not
inside the archive.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = 1
STATUS_VALUES = (
    "IDEA",
    "PREREGISTERED",
    "PRIMARY_RESEARCH",
    "ROBUSTNESS",
    "INDEPENDENT_AUDIT",
    "HOLDOUT",
    "RESEARCH_QUALIFIED",
    "SHADOW",
    "PAPER",
    "DEMO",
    "REJECTED",
    "ARCHIVED",
)
GATE_STATES = (
    "PASSED",
    "FAILED",
    "NOT_OPENED",
    "NOT_PERFORMED",
    "UNOPENED",
    "UNVERIFIED",
)
SHA256_RE = re.compile(r"^(?:sha256:)?([0-9a-f]{64})$")
DECIMAL_RE = re.compile(r"^-?\d+(?:\.\d+)?$")
CANDIDATE_ID_RE = re.compile(r"^[A-Za-z][A-Za-z0-9]*V\d+$")
ARTIFACT_ID_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
DATASET_ID_RE = re.compile(r"^[a-z0-9][a-z0-9_]*$")
GIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
PROVENANCE_RECORDED = "recorded"
PROVENANCE_LEGACY_UNRECORDED = "legacy_unrecorded"
PROVENANCE_VALUES = (PROVENANCE_RECORDED, PROVENANCE_LEGACY_UNRECORDED)
SECRET_PATH_RE = re.compile(
    r"(?:^|/)(?:"
    r"\.env(?:\..*)?|"
    r"operator\.token|"
    r"operator\.env|"
    r".*credentials.*|"
    r"bot\.json"
    r")$",
    re.IGNORECASE,
)
SECRET_DIR_RE = re.compile(r"(?:^|/)(?:secrets|runtime-data|config)(?:/|$)")
SECRET_CONTENT_RE = re.compile(
    r"BEGIN (?:OPENSSH |RSA |EC )?PRIVATE KEY|\"apiSecret\"\s*:|\"apiKey\"\s*:"
)
TRAVERSAL_RE = re.compile(r"(?:^|/)\.\.(?:/|$)")


class ArtifactError(Exception):
    pass


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalize_sha(value: str) -> str:
    match = SHA256_RE.match(str(value).strip())
    if not match:
        raise ArtifactError(f"invalid sha256: {value}")
    return match.group(1)


def format_sha(value: str) -> str:
    return f"sha256:{normalize_sha(value)}"


def now_utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ArtifactError(f"read {path}: {exc}") from exc


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=False) + "\n")


def require_mapping(value: Any, name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ArtifactError(f"{name} must be an object")
    return value


def require_str(value: Any, name: str) -> str:
    if not isinstance(value, str) or not value:
        raise ArtifactError(f"{name} must be a non-empty string")
    return value


def require_bool(value: Any, name: str) -> bool:
    if not isinstance(value, bool):
        raise ArtifactError(f"{name} must be a boolean")
    return value


def require_list(value: Any, name: str) -> list[Any]:
    if not isinstance(value, list) or not value:
        raise ArtifactError(f"{name} must be a non-empty array")
    return value


def artifact_root(explicit: str | None = None) -> Path:
    raw = explicit or os.environ.get("RESEARCH_ARTIFACT_ROOT")
    if not raw:
        raise ArtifactError("set RESEARCH_ARTIFACT_ROOT or pass --root")
    path = Path(raw).expanduser().resolve()
    path.mkdir(parents=True, exist_ok=True)
    (path / "staging").mkdir(exist_ok=True)
    (path / "objects" / "sha256").mkdir(parents=True, exist_ok=True)
    (path / "datasets" / "sha256").mkdir(parents=True, exist_ok=True)
    (path / "ids").mkdir(exist_ok=True)
    return path


def object_path(root: Path, digest: str, kind: str) -> Path:
    digest = normalize_sha(digest)
    return root / kind / "sha256" / digest[:2] / (
        f"{digest}.tar.zst" if kind == "objects" else digest
    )


def id_path(root: Path, artifact_id: str) -> Path:
    return root / "ids" / f"{artifact_id}.json"


def same_device(left: Path, right: Path) -> bool:
    return os.stat(left).st_dev == os.stat(right).st_dev


def fsync_file(path: Path) -> None:
    with path.open("rb") as handle:
        os.fsync(handle.fileno())
    directory = os.open(str(path.parent), os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def publish_bytes(dest: Path, source: Path, digest: str) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists():
        existing = sha256_file(dest)
        if existing != digest:
            raise ArtifactError(
                f"refusing to replace {dest} (have {existing}, new {digest})"
            )
        return
    tmp = dest.with_name(f".{dest.name}.{os.getpid()}.tmp")
    try:
        shutil.copy2(source, tmp)
        os.chmod(tmp, stat.S_IRUSR | stat.S_IWUSR | stat.S_IRGRP | stat.S_IROTH)
        fsync_file(tmp)
        got = sha256_file(tmp)
        if got != digest:
            raise ArtifactError(f"staged hash {got} != expected {digest}")
        os.replace(tmp, dest)
        fsync_file(dest)
    finally:
        if tmp.exists():
            tmp.unlink()


def suspect_secret(path: Path, repo_relative: str) -> str | None:
    rel = repo_relative.replace("\\", "/")
    if SECRET_PATH_RE.search(rel) or SECRET_DIR_RE.search(rel):
        return f"secret path {rel}"
    try:
        sample = path.read_bytes()[:1 << 16]
    except OSError:
        return None
    if b"\0" in sample:
        return None
    text = sample.decode("utf-8", "ignore")
    if SECRET_CONTENT_RE.search(text):
        return f"secret content in {rel}"
    return None


def validate_member_name(name: str) -> str:
    cleaned = name.replace("\\", "/").lstrip("./")
    if cleaned in ("", "."):
        return "."
    if cleaned.startswith("/") or TRAVERSAL_RE.search("/" + cleaned + "/"):
        raise ArtifactError(f"refusing archive path {name}")
    if cleaned.startswith("ids/") or cleaned == "SHA256" or cleaned.endswith(".sha256"):
        raise ArtifactError(f"archive must not contain outer digest path {name}")
    return cleaned


def validate_window(window: dict[str, Any]) -> None:
    start = require_str(window.get("start_utc"), "experiment_window.start_utc")
    end = require_str(window.get("end_exclusive_utc"), "experiment_window.end_exclusive_utc")
    semantics = require_str(window.get("semantics"), "experiment_window.semantics")
    if semantics != "end_exclusive":
        raise ArtifactError("experiment_window.semantics must be end_exclusive")
    if not start.endswith("Z") or not end.endswith("Z"):
        raise ArtifactError("experiment window times must be UTC Zulu")
    if start >= end:
        raise ArtifactError("experiment window start must be before end_exclusive")


def validate_gate(gate: Any, name: str) -> dict[str, Any]:
    mapping = require_mapping(gate, name)
    state = require_str(mapping.get("state"), f"{name}.state")
    if state not in GATE_STATES:
        raise ArtifactError(f"{name}.state {state} is not a documented gate state")
    require_str(mapping.get("note"), f"{name}.note")
    if name.endswith("holdout") and state == "PASSED":
        raise ArtifactError("an unopened or reused holdout cannot be recorded as PASSED")
    if state == "UNOPENED" and mapping.get("used_as_validation") is True:
        raise ArtifactError("UNOPENED holdout cannot be described as validation")
    return mapping


def validate_results(results: dict[str, Any]) -> None:
    for key, value in results.items():
        if key in {"completed_trades"}:
            if not isinstance(value, int) or value < 0:
                raise ArtifactError("results.completed_trades must be a non-negative integer")
            continue
        if isinstance(value, bool) or value is None:
            continue
        if isinstance(value, (int, float)):
            raise ArtifactError(f"results.{key} must be an exact decimal string, not a number")
        if isinstance(value, str) and not DECIMAL_RE.match(value):
            if key.endswith("_note"):
                continue
            raise ArtifactError(f"results.{key} must be an exact decimal string")


def require_git_sha(value: Any, name: str) -> str:
    revision = require_str(value, name)
    if not GIT_SHA_RE.fullmatch(revision):
        raise ArtifactError(f"{name} must be a 40-character SHA")
    return revision


def absent_or_null(mapping: dict[str, Any], key: str) -> bool:
    return key not in mapping or mapping[key] is None


def validate_scored_source(scored: dict[str, Any]) -> dict[str, Any]:
    provenance = scored.get("provenance", PROVENANCE_RECORDED)
    if provenance not in PROVENANCE_VALUES:
        raise ArtifactError(
            f"scored_source.provenance must be one of {', '.join(PROVENANCE_VALUES)}"
        )
    require_str(scored.get("reconstruction"), "scored_source.reconstruction")
    if provenance == PROVENANCE_LEGACY_UNRECORDED:
        if not absent_or_null(scored, "revision"):
            raise ArtifactError(
                "legacy_unrecorded scored_source.revision must be omitted; "
                "an evidence commit is not a scoring SHA"
            )
        if not absent_or_null(scored, "dirty"):
            raise ArtifactError(
                "legacy_unrecorded scored_source.dirty must be omitted; "
                "the original clean/dirty state was not recorded"
            )
        if not absent_or_null(scored, "command"):
            raise ArtifactError(
                "legacy_unrecorded scored_source.command must be omitted; "
                "do not invent a verified scoring command"
            )
        require_git_sha(scored.get("ledger_revision"), "scored_source.ledger_revision")
        contemporaneous = require_str(
            scored.get("contemporaneous_command"),
            "scored_source.contemporaneous_command",
        )
        if "research_cli" in contemporaneous:
            raise ArtifactError(
                "legacy contemporaneous_command cannot name research_cli; "
                "that crate did not exist when the ledgers were first tracked"
            )
        return scored
    require_git_sha(scored.get("revision"), "scored_source.revision")
    dirty = require_bool(scored.get("dirty"), "scored_source.dirty")
    if dirty:
        raise ArtifactError("scored_source.dirty must be false for a recorded scoring run")
    require_str(scored.get("command"), "scored_source.command")
    return scored


def validate_manifest(manifest: Any) -> dict[str, Any]:
    data = require_mapping(manifest, "manifest")
    if data.get("schema_version") != SCHEMA_VERSION:
        raise ArtifactError(f"schema_version must be {SCHEMA_VERSION}")
    candidate_id = require_str(data.get("candidate_id"), "candidate_id")
    if not CANDIDATE_ID_RE.match(candidate_id):
        raise ArtifactError("candidate_id must look like NameV1")
    require_str(data.get("candidate_version"), "candidate_version")
    status = require_list(data.get("status"), "status")
    unknown = [item for item in status if item not in STATUS_VALUES]
    if unknown:
        raise ArtifactError(f"unknown status values: {unknown}")
    if "REJECTED" in status and "ARCHIVED" not in status:
        raise ArtifactError("REJECTED records must also be ARCHIVED")
    validate_window(require_mapping(data.get("experiment_window"), "experiment_window"))
    spec = require_mapping(data.get("spec"), "spec")
    normalize_sha(require_str(spec.get("sha256"), "spec.sha256"))
    normalize_sha(require_str(spec.get("experiment_plan_sha256"), "spec.experiment_plan_sha256"))
    require_str(spec.get("path"), "spec.path")
    require_str(spec.get("experiment_plan_path"), "spec.experiment_plan_path")
    validate_scored_source(require_mapping(data.get("scored_source"), "scored_source"))
    datasets = require_list(data.get("datasets"), "datasets")
    for index, item in enumerate(datasets):
        row = require_mapping(item, f"datasets[{index}]")
        dataset_id = require_str(row.get("id"), f"datasets[{index}].id")
        if not DATASET_ID_RE.match(dataset_id):
            raise ArtifactError(f"datasets[{index}].id is not a stable dataset id")
        normalize_sha(require_str(row.get("sha256"), f"datasets[{index}].sha256"))
        require_str(row.get("content_hash"), f"datasets[{index}].content_hash")
        require_str(row.get("content_hash_kind"), f"datasets[{index}].content_hash_kind")
        require_str(row.get("role"), f"datasets[{index}].role")
    validate_results(require_mapping(data.get("results"), "results"))
    require_mapping(data.get("cost_assumptions"), "cost_assumptions")
    artifact = require_mapping(data.get("artifact"), "artifact")
    artifact_id = require_str(artifact.get("id"), "artifact.id")
    if not ARTIFACT_ID_RE.match(artifact_id):
        raise ArtifactError("artifact.id must be a stable logical id")
    if "ARCHIVED" in status:
        normalize_sha(require_str(artifact.get("sha256"), "artifact.sha256"))
    gates = require_mapping(data.get("gates"), "gates")
    for name in ("primary", "robustness", "independent_audit", "holdout"):
        validate_gate(gates.get(name), f"gates.{name}")
    if gates["holdout"]["state"] == "UNOPENED" and "HOLDOUT" in status:
        raise ArtifactError("status HOLDOUT cannot be used when the holdout is UNOPENED")
    lifecycle = require_mapping(data.get("lifecycle"), "lifecycle")
    require_str(lifecycle.get("reached"), "lifecycle.reached")
    require_bool(lifecycle.get("preregistered_on_master"), "lifecycle.preregistered_on_master")
    require_str(lifecycle.get("preregistration_note"), "lifecycle.preregistration_note")
    return data


def validate_manifest_file(path: Path) -> dict[str, Any]:
    return validate_manifest(load_json(path))


def repo_root_from(start: Path) -> Path:
    output = subprocess.check_output(
        ["git", "-C", str(start), "rev-parse", "--show-toplevel"],
        text=True,
    )
    return Path(output.strip())


def scored_rev_for_index(scored: dict[str, Any]) -> str:
    if scored.get("provenance") == PROVENANCE_LEGACY_UNRECORDED:
        return "unrecorded"
    return scored["revision"][:12]


def generate_index(manifests: Iterable[dict[str, Any]]) -> str:
    rows = []
    for item in sorted(manifests, key=lambda row: (row["candidate_id"], row["candidate_version"])):
        status = "+".join(item["status"])
        window = item["experiment_window"]
        artifact = item.get("artifact") or {}
        rows.append(
            "| {id} | {ver} | {status} | {start} → {end} | `{rev}` | `{art}` | `{sha}` |".format(
                id=item["candidate_id"],
                ver=item["candidate_version"],
                status=status,
                start=window["start_utc"],
                end=window["end_exclusive_utc"],
                rev=scored_rev_for_index(item["scored_source"]),
                art=artifact.get("id", ""),
                sha=(artifact.get("sha256") or "")[:12],
            )
        )
    body = "\n".join(rows) if rows else "| *(none)* | | | | | | |"
    return (
        "# Research index\n\n"
        "Generated from `research/candidates/*/manifest.json`. "
        "Do not edit by hand; run `scripts/research_artifacts.py generate-index`.\n\n"
        "| Candidate | Version | Status | Window (UTC, end exclusive) | Scored rev | Artifact | Archive SHA |\n"
        "| --- | --- | --- | --- | --- | --- | --- |\n"
        f"{body}\n"
    )


def iter_manifests(repo: Path) -> list[tuple[Path, dict[str, Any]]]:
    found = []
    root = repo / "research" / "candidates"
    if not root.exists():
        return found
    for path in sorted(root.glob("*/manifest.json")):
        found.append((path, validate_manifest_file(path)))
    return found


def cmd_validate_manifest(args: argparse.Namespace) -> int:
    validate_manifest_file(Path(args.path))
    print(f"valid {args.path}")
    return 0


def cmd_generate_index(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    text = generate_index(item for _, item in iter_manifests(repo))
    dest = repo / "research" / "INDEX.md"
    dest.write_text(text)
    print(f"wrote {dest}")
    return 0


def cmd_check_index(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    expected = generate_index(item for _, item in iter_manifests(repo))
    actual = (repo / "research" / "INDEX.md").read_text()
    if actual != expected:
        raise ArtifactError("research/INDEX.md does not match candidate manifests")
    print("index matches manifests")
    return 0


def publish_dataset(root: Path, source: Path) -> str:
    if not source.is_file():
        raise ArtifactError(f"dataset missing: {source}")
    digest = sha256_file(source)
    dest = object_path(root, digest, "datasets")
    publish_bytes(dest, source, digest)
    return digest


def resolve_dataset(root: Path, digest: str) -> Path:
    dest = object_path(root, digest, "datasets")
    if not dest.is_file():
        raise ArtifactError(f"dataset {digest} is absent")
    got = sha256_file(dest)
    if got != normalize_sha(digest):
        raise ArtifactError(f"dataset {digest} has hash {got}")
    return dest


def create_git_bundle(repo: Path, dest: Path, base: str, tip: str) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    listed = subprocess.check_output(
        ["git", "-C", str(repo), "rev-list", "--count", f"{base}..{tip}"],
        text=True,
    ).strip()
    if listed == "0":
        raise ArtifactError(
            f"git range {base}..{tip} is empty; cannot reconstruct scored source"
        )
    tip_ref = "refs/research-artifacts/bundle-tip"
    subprocess.check_call(["git", "-C", str(repo), "update-ref", tip_ref, tip])
    try:
        try:
            subprocess.check_call(
                [
                    "git",
                    "-C",
                    str(repo),
                    "bundle",
                    "create",
                    str(dest),
                    tip_ref,
                    "--not",
                    base,
                ],
                stdout=subprocess.DEVNULL,
            )
        except subprocess.CalledProcessError as exc:
            raise ArtifactError(f"git bundle create failed: {exc}") from exc
        try:
            subprocess.check_call(
                ["git", "-C", str(repo), "bundle", "verify", str(dest)],
                stdout=subprocess.DEVNULL,
            )
        except subprocess.CalledProcessError as exc:
            raise ArtifactError(f"git bundle verify failed: {exc}") from exc
    finally:
        subprocess.call(
            ["git", "-C", str(repo), "update-ref", "-d", tip_ref],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def pack(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    root = artifact_root(args.root)
    allowlist = load_json(Path(args.allowlist))
    artifact_id = require_str(allowlist.get("artifact_id"), "allowlist.artifact_id")
    if not ARTIFACT_ID_RE.match(artifact_id):
        raise ArtifactError("allowlist.artifact_id is not a stable logical id")
    files = list(allowlist.get("files") or [])
    for pattern in allowlist.get("globs") or []:
        glob_pattern = pattern + "/*" if pattern.endswith("/**") else pattern
        for path in sorted(repo.glob(glob_pattern)):
            if path.is_file():
                files.append({"path": str(path.relative_to(repo)), "role": "glob"})
    if not files:
        raise ArtifactError("allowlist has no files")
    datasets = require_list(allowlist.get("datasets"), "allowlist.datasets")
    source = require_mapping(allowlist.get("source"), "allowlist.source")

    staging = Path(tempfile.mkdtemp(prefix="pack-", dir=root / "staging"))
    bundle_dir = staging / "bundle"
    bundle_dir.mkdir()
    try:
        copied = []
        seen_paths: set[str] = set()
        for item in files:
            row = require_mapping(item, "allowlist.files[]")
            rel = require_str(row.get("path"), "files[].path")
            if rel in seen_paths:
                continue
            seen_paths.add(rel)
            validate_member_name(rel)
            src = (repo / rel).resolve()
            if not str(src).startswith(str(repo) + os.sep):
                raise ArtifactError(f"file escapes repo: {rel}")
            if not src.is_file():
                raise ArtifactError(f"missing allowlisted file {rel}")
            reason = suspect_secret(src, rel)
            if reason:
                raise ArtifactError(f"refusing to archive {reason}")
            dest = bundle_dir / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dest)
            copied.append({"path": rel, "sha256": sha256_file(src), "role": row.get("role", "")})

        dataset_refs = []
        for item in datasets:
            row = require_mapping(item, "allowlist.datasets[]")
            dataset_id = require_str(row.get("id"), "datasets[].id")
            rel = require_str(row.get("path"), "datasets[].path")
            src = repo / rel
            reason = suspect_secret(src, rel)
            if reason:
                raise ArtifactError(f"refusing to archive {reason}")
            digest = publish_dataset(root, src)
            expected = row.get("sha256")
            if expected and normalize_sha(expected) != digest:
                raise ArtifactError(
                    f"dataset {dataset_id} hash {digest} != allowlist {expected}"
                )
            dataset_refs.append(
                {
                    "id": dataset_id,
                    "path": rel,
                    "sha256": digest,
                    "content_hash": row.get("content_hash"),
                    "content_hash_kind": row.get("content_hash_kind"),
                    "role": row.get("role"),
                }
            )

        write_json(bundle_dir / "dataset-refs.json", dataset_refs)
        write_json(bundle_dir / "copied-files.json", copied)
        write_json(
            bundle_dir / "commands.json",
            allowlist.get("commands")
            or {"score": source.get("command", "")},
        )
        bundle_path = bundle_dir / "source.bundle"
        create_git_bundle(
            repo,
            bundle_path,
            require_str(source.get("base"), "source.base"),
            require_str(source.get("tip"), "source.tip"),
        )
        (bundle_dir / "SOURCE.md").write_text(
            require_str(source.get("reconstruction"), "source.reconstruction")
        )
        pin = allowlist.get("pin") or {}
        write_json(bundle_dir / "pin.json", pin)

        tar_tmp = staging / "archive.tar.zst"
        subprocess.check_call(
            [
                "tar",
                "--zstd",
                "-C",
                str(bundle_dir),
                "-cf",
                str(tar_tmp),
                "--sort=name",
                "--mtime=UTC 1970-01-01",
                "--owner=0",
                "--group=0",
                "--numeric-owner",
                ".",
            ]
        )
        fsync_file(tar_tmp)
        digest = sha256_file(tar_tmp)
        dest = object_path(root, digest, "objects")
        publish_bytes(dest, tar_tmp, digest)

        record = {
            "artifact_id": artifact_id,
            "sha256": digest,
            "created_utc": now_utc(),
        }
        existing_id = id_path(root, artifact_id)
        if existing_id.exists():
            previous = load_json(existing_id)
            if normalize_sha(previous.get("sha256", "")) != digest:
                raise ArtifactError(
                    f"artifact id {artifact_id} already points to {previous.get('sha256')}"
                )
        write_json(existing_id, record)
        print(f"artifact_id={artifact_id}")
        print(f"sha256={digest}")
        print(f"object={dest}")
        return 0
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def load_artifact_record(root: Path, artifact_id: str) -> dict[str, Any]:
    path = id_path(root, artifact_id)
    if not path.is_file():
        raise ArtifactError(f"unknown artifact id {artifact_id}")
    record = load_json(path)
    normalize_sha(record.get("sha256", ""))
    return record


def verify_object(root: Path, digest: str) -> Path:
    dest = object_path(root, digest, "objects")
    if not dest.is_file():
        raise ArtifactError(f"object {digest} is absent")
    got = sha256_file(dest)
    if got != normalize_sha(digest):
        raise ArtifactError(f"object {digest} has hash {got}")
    return dest


def open_tar(archive: Path) -> tarfile.TarFile:
    try:
        header = archive.read_bytes()[:4]
        if header == b"\x28\xb5\x2f\xfd":
            raw = subprocess.check_output(["zstd", "-d", "-c", str(archive)])
            return tarfile.open(fileobj=io.BytesIO(raw), mode="r:")
        return tarfile.open(archive, mode="r:*")
    except (tarfile.TarError, OSError, subprocess.CalledProcessError) as exc:
        raise ArtifactError(f"cannot read archive {archive}: {exc}") from exc


def extract_archive(archive: Path, dest: Path) -> None:
    dest.mkdir(parents=True, exist_ok=True)
    dest_resolved = dest.resolve()
    with open_tar(archive) as handle:
        for member in handle.getmembers():
            name = validate_member_name(member.name)
            target = (dest / name).resolve()
            if not str(target).startswith(str(dest_resolved) + os.sep) and target != dest_resolved:
                raise ArtifactError(f"refusing extract path {member.name}")
            if member.issym() or member.islnk():
                raise ArtifactError(f"refusing link member {member.name}")
        try:
            handle.extractall(dest, filter="data")
        except (tarfile.TarError, ValueError, OSError) as exc:
            raise ArtifactError(f"refusing archive extraction: {exc}") from exc


def verify_bundle_dir(root: Path, bundle_dir: Path) -> list[dict[str, Any]]:
    refs = load_json(bundle_dir / "dataset-refs.json")
    if not isinstance(refs, list):
        raise ArtifactError("dataset-refs.json must be an array")
    for row in refs:
        mapping = require_mapping(row, "dataset-ref")
        resolve_dataset(root, require_str(mapping.get("sha256"), "dataset-ref.sha256"))
    copied = load_json(bundle_dir / "copied-files.json")
    for row in copied:
        mapping = require_mapping(row, "copied-file")
        path = bundle_dir / require_str(mapping.get("path"), "copied-file.path")
        if sha256_file(path) != normalize_sha(require_str(mapping.get("sha256"), "copied-file.sha256")):
            raise ArtifactError(f"restored file hash mismatch {mapping.get('path')}")
    if not (bundle_dir / "source.bundle").is_file():
        raise ArtifactError("source.bundle missing")
    if not (bundle_dir / "SOURCE.md").is_file():
        raise ArtifactError("SOURCE.md missing")
    return refs


def cmd_verify(args: argparse.Namespace) -> int:
    root = artifact_root(args.root)
    record = load_artifact_record(root, args.artifact_id)
    archive = verify_object(root, record["sha256"])
    tmp = Path(tempfile.mkdtemp(prefix="verify-", dir=root / "staging"))
    try:
        extract_archive(archive, tmp)
        verify_bundle_dir(root, tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print(f"verified {args.artifact_id} sha256={record['sha256']}")
    return 0


def materialize_datasets(root: Path, bundle_dir: Path, dest: Path) -> None:
    refs = load_json(bundle_dir / "dataset-refs.json")
    for row in refs:
        digest = normalize_sha(row["sha256"])
        source = resolve_dataset(root, digest)
        target = dest / row["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        if sha256_file(target) != digest:
            raise ArtifactError(f"restore wrote wrong bytes for {row['path']}")


def cmd_restore(args: argparse.Namespace) -> int:
    root = artifact_root(args.root)
    dest = Path(args.dest).resolve()
    if dest.exists() and any(dest.iterdir()):
        raise ArtifactError(f"restore destination must be empty: {dest}")
    dest.mkdir(parents=True, exist_ok=True)
    record = load_artifact_record(root, args.artifact_id)
    archive = verify_object(root, record["sha256"])
    extract_archive(archive, dest)
    verify_bundle_dir(root, dest)
    materialize_datasets(root, dest, dest)
    print(f"restored {args.artifact_id} -> {dest}")
    return 0


def copy_store_tree(src_root: Path, dest_root: Path, digest: str) -> None:
    dest_root.mkdir(parents=True, exist_ok=True)
    (dest_root / "staging").mkdir(exist_ok=True)
    archive = verify_object(src_root, digest)
    publish_bytes(object_path(dest_root, digest, "objects"), archive, normalize_sha(digest))
    tmp = Path(tempfile.mkdtemp(prefix="backup-", dir=src_root / "staging"))
    try:
        extract_archive(archive, tmp)
        refs = verify_bundle_dir(src_root, tmp)
        for row in refs:
            dataset_digest = normalize_sha(row["sha256"])
            source = resolve_dataset(src_root, dataset_digest)
            publish_bytes(
                object_path(dest_root, dataset_digest, "datasets"),
                source,
                dataset_digest,
            )
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def cmd_backup(args: argparse.Namespace) -> int:
    root = artifact_root(args.root)
    backup = artifact_root(args.backup_root)
    if same_device(root, backup):
        raise ArtifactError(
            "backup root is on the same physical device as RESEARCH_ARTIFACT_ROOT; "
            "two directories on one disk are not a backup"
        )
    record = load_artifact_record(root, args.artifact_id)
    copy_store_tree(root, backup, record["sha256"])
    write_json(id_path(backup, args.artifact_id), record)
    print(f"backup {args.artifact_id} sha256={record['sha256']} -> {backup}")
    print(f"devices primary={os.stat(root).st_dev} backup={os.stat(backup).st_dev}")
    return 0


def cmd_devices(args: argparse.Namespace) -> int:
    root = artifact_root(args.root)
    backup = Path(args.backup_root).expanduser().resolve() if args.backup_root else None
    print(f"primary={root} dev={os.stat(root).st_dev}")
    if backup:
        backup.mkdir(parents=True, exist_ok=True)
        print(f"backup={backup} dev={os.stat(backup).st_dev}")
        print("same_device" if same_device(root, backup) else "different_device")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    validate = sub.add_parser("validate-manifest")
    validate.add_argument("path")
    validate.set_defaults(func=cmd_validate_manifest)

    generate = sub.add_parser("generate-index")
    generate.add_argument("--repo", default=".")
    generate.set_defaults(func=cmd_generate_index)

    check = sub.add_parser("check-index")
    check.add_argument("--repo", default=".")
    check.set_defaults(func=cmd_check_index)

    pack_cmd = sub.add_parser("pack")
    pack_cmd.add_argument("--allowlist", required=True)
    pack_cmd.add_argument("--repo", default=".")
    pack_cmd.add_argument("--root")
    pack_cmd.set_defaults(func=pack)

    verify = sub.add_parser("verify")
    verify.add_argument("artifact_id")
    verify.add_argument("--root")
    verify.set_defaults(func=cmd_verify)

    restore = sub.add_parser("restore")
    restore.add_argument("artifact_id")
    restore.add_argument("--dest", required=True)
    restore.add_argument("--root")
    restore.set_defaults(func=cmd_restore)

    backup = sub.add_parser("backup")
    backup.add_argument("artifact_id")
    backup.add_argument("--root")
    backup.add_argument("--backup-root", required=True)
    backup.set_defaults(func=cmd_backup)

    devices = sub.add_parser("devices")
    devices.add_argument("--root")
    devices.add_argument("--backup-root")
    devices.set_defaults(func=cmd_devices)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.func(args)
    except ArtifactError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
