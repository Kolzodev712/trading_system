# Research candidate manifest schema v1

Machine-readable record: `research/candidates/<candidate_id>/manifest.json`.
Human-readable twins: `SPEC.md` and `DISPOSITION.md` in the same directory.
Shared dataset metadata: `research/datasets/<dataset_id>.json`.

`schema_version` is the integer `1`. Unknown fields are rejected by the validator
only when they break required types; extra fields are allowed so a later
additive note does not require a schema bump. Required fields are not optional.

## Status values

Use one or more of these tokens in `status` (array, stable spellings):

```text
IDEA
PREREGISTERED
PRIMARY_RESEARCH
ROBUSTNESS
INDEPENDENT_AUDIT
HOLDOUT
RESEARCH_QUALIFIED
SHADOW
PAPER
DEMO
REJECTED
ARCHIVED
```

Lifecycle on master:

```text
IDEA → PREREGISTERED → PRIMARY_RESEARCH
  → ROBUSTNESS → INDEPENDENT_AUDIT → HOLDOUT
  → RESEARCH_QUALIFIED → SHADOW → PAPER → DEMO
```

A failure at any research gate is recorded as `REJECTED` and `ARCHIVED`.
`REJECTED` without `ARCHIVED` is invalid. `HOLDOUT` is invalid while the
holdout gate is `UNOPENED`.

For **future** candidates, commit `SPEC.md` and the experiment plan to
`master` as `PREREGISTERED` **before** scoring. The spec is then locked. A
rule change after scoring is a new candidate version or a new experiment.

## Required fields

| Field | Rule |
| --- | --- |
| `candidate_id` | Strategy-style id, `NameV1` |
| `candidate_version` | Non-empty string |
| `status` | Non-empty array of the tokens above |
| `lifecycle.reached` | Furthest opened research stage |
| `lifecycle.preregistered_on_master` | Boolean. Do not set true retrospectively |
| `lifecycle.preregistration_note` | Honest chronology |
| `experiment_window.start_utc` / `end_exclusive_utc` | UTC `Z`, end exclusive |
| `experiment_window.semantics` | Must be `end_exclusive` |
| `spec.path` / `spec.sha256` | Human spec file and SHA-256 |
| `spec.experiment_plan_path` / `spec.experiment_plan_sha256` | Frozen plan |
| `scored_source.revision` | 40-character Git SHA that produced the numbers |
| `scored_source.dirty` | Must be false for a scored run |
| `scored_source.command` | Exact scoring command |
| `scored_source.reconstruction` | How to rebuild that source without the branch |
| `datasets[]` | `id`, file `sha256`, engine `content_hash`, `content_hash_kind`, `role` |
| `results` | Decimal PnL and costs as exact strings. Integers only for counts |
| `cost_assumptions` | Named research scenarios, not venue fee quotes |
| `artifact.id` | Logical id, not a host path |
| `artifact.sha256` | SHA-256 of the outer `.tar.zst`. Required when `ARCHIVED` |
| `gates.primary` / `robustness` / `independent_audit` / `holdout` | See below |

## Gate states

`PASSED`, `FAILED`, `NOT_OPENED`, `NOT_PERFORMED`, `UNOPENED`, `UNVERIFIED`.

An `UNOPENED` holdout cannot set `used_as_validation=true` and cannot be
`PASSED`. Reusing a previously opened holdout is not validation.

## Hashes

File integrity uses lowercase hex SHA-256, optionally prefixed `sha256:`.
Research-engine content hashes (canonical payload, not raw bytes) are stored
separately as `content_hash` and must not be treated as the artifact-store
file digest.

The outer archive digest lives in Git and in the artifact-store id record.
It must not appear inside the archive.
