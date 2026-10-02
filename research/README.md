# Research records and local artifacts

Keep a compact, permanent record on `master`. Keep candidate implementations
and generated ledgers off `master` until they pass the research gates. Preserve
complete failed experiments in a local artifact store outside Git.

## Layout

```text
research/candidates/<candidate_id>/
  SPEC.md
  experiment.json
  DISPOSITION.md
  manifest.json
  archive-allowlist.json

research/datasets/<dataset_id>.json
research/INDEX.md          # generated from manifests
research/LIVE_STRATEGIES.md
research/schema/manifest-v1.md
```

`INDEX.md` is generated. `just research-index-check` fails if it drifts.
Legacy rejected screens that predate this process use
`scored_source.provenance=legacy_unrecorded`. Their ledger commit is not a
scoring SHA.

## Future candidates

1. Write `SPEC.md` and the experiment plan.
2. Commit them to `master` with status `PREREGISTERED` **before** scoring.
3. Lock the spec. A rule change after scoring is a new version or experiment.
4. Score on a lean branch. Do not commit generated `data/**/backtest` files.
5. Pack the local archive, copy it to a **different physical device**, restore
   from that copy, then update `DISPOSITION.md` / `manifest.json` on `master`.

## Artifact store

Set `RESEARCH_ARTIFACT_ROOT` to a directory **outside** this repository.

```text
$RESEARCH_ARTIFACT_ROOT/
  staging/
  objects/sha256/<aa>/<sha256>.tar.zst
  datasets/sha256/<aa>/<sha256>
  ids/<artifact_id>.json
```

```sh
export RESEARCH_ARTIFACT_ROOT="$HOME/research-artifacts"
python3 scripts/research_artifacts.py pack --allowlist research/candidates/<id>/archive-allowlist.json
python3 scripts/research_artifacts.py verify <artifact_id>
python3 scripts/research_artifacts.py restore <artifact_id> --dest /tmp/restore-<id>
python3 scripts/research_artifacts.py backup <artifact_id> --backup-root /mnt/other-disk/research-artifacts
```

`backup` refuses a destination on the same `st_dev` as the primary store. Two
folders on one disk are not a backup. v1 is an external drive or a second
local machine; there is no S3 API.

The packer:

- copies an explicit allowlist
- stores large datasets once by file SHA-256
- writes a Git bundle plus reconstruction notes so a deleted branch is not
  required to rebuild the scored source
- fails if a suspected secret path or private-key / API-secret blob is included
- publishes through `staging` and will not replace a different object
- is idempotent for the same id and bytes; a conflicting id is an error

A SHA is integrity, not preservation. Qualification requires a verified copy
on a different physical device and a fresh-directory restore from that copy.

## Git guard

`scripts/research_git_guard.py` blocks new generated datasets, ledgers, and
reports before they are committed. Existing tracked files stay tracked. Run it
before any research commit (`just research-git-guard`). `.githooks/pre-commit`
calls the same check if `core.hooksPath` is already `.githooks`; this repo
does not change your Git config.

## What does not go to master

Rejected models, candidate-specific CLI commands, and candidate-specific
replay stay off `master`. `runner` and `trading_engine` must not depend on
`strategy_core` or the research crates.

Legacy DonchianDailyV1 and BTCPriceMALongFlatV1 *research* screens are
`REJECTED+ARCHIVED` with local artifacts. Shared loaders live in
`daily_dataset`, `market_inputs`, and `simulation`. The live factory types
`Donchian` and `BuyLowSellHigh`, and the Candidate #2 `strategy_core` model,
are decided in `LIVE_STRATEGIES.md`.
