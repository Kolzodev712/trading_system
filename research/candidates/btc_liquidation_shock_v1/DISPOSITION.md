# BTCLiquidationShockV1 disposition

Final for candidate version 1. This does not change the frozen rule in
`SPEC.md` or `experiment.json`. Those files match the scored bytes at
`8e20b5f53b83101c2dba05e823d9ad69d91ccd9e`.

- Candidate: `BTCLiquidationShockV1` version `1`
- Status: `REJECTED` + `ARCHIVED`
- Window: `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)` (UTC, end exclusive)
- Primary: 3σ reversal / 6 completed hours / ±0.001 BTC
- Completed trades: 416
- Zero-cost reversal net: `-13.9696470237563`
- Cost A reversal net: `-57.3483451072483`
- Higher-cost reversal net: `-122.4164020528563`
- 300-second-delay reversal net: `-58.2887238598203`
- Continuation placebo Cost A: `-29.4090476927517`
- Entry-year Cost A nets: 2023 `-3.0370205363058`, 2024 `-25.2664317449723`, 2025 `-29.0448928259702`
- Robustness grid: `NOT_OPENED` (primary promotion failed)
- Independent audit: `NOT_PERFORMED`
- 2026 holdout: `UNOPENED` (sealed; not validation)
- Shadow / Paper / Demo: not opened

The rejected claim is this hourly-return shock proxy and this frozen rule,
not a later model.

## Provenance

The run manifest at `data/btc_liquidation_shock_v1/backtest/manifest.json`
records `revision=8e20b5f53b83101c2dba05e823d9ad69d91ccd9e` and `dirty=false`.
`SPEC.md` and `experiment.json` first appear in that commit. The scoring
command is `cargo run -p research_cli --quiet -- btc-liquidation-shock run`
(defaults). Ledgers were committed afterwards as
`036502dd3d8b951f3f49b363a38add694815ea32`. Code between those two commits
does not change.

This spec was **not** preregistered on `master`. It was frozen on
`feat/btc-liquidation-shock-v1` in the same commit that first made the
primary rule scorable. That chronology is recorded; it is not rewritten as
a master preregistration.

Dataset file SHA-256 values are the raw bytes in the artifact store.
`content_hash` values are the research-engine dataset hashes from the run
manifest (canonical payload, not raw files). Both matched the files present
when this record was written.

The complete primary-run archive and source Git bundle live outside Git.

- Artifact id: `btc_liquidation_shock_v1-v1-primary`
- Outer SHA-256: `bcf6063039bb3392817e59f1e1fd1f5bafa3daede3c822d1d8f615e62a85b048`
- Primary store: `RESEARCH_ARTIFACT_ROOT` (this machine: `/home/kolzo-dev/research-artifacts`)
- Second physical copy: Samsung 970 EVO Plus (`nvme1n1p5`) at `/media/kolzo-dev/DA8EAF938EAF672B/research-artifacts`
- Reconstruct source: `git fetch source.bundle 'refs/*:refs/restore/*'` then checkout `8e20b5f53b83101c2dba05e823d9ad69d91ccd9e`. The bundle requires master `f5d1e0f183a1f195634e7b767aab03996167cc26`.
- Numerical replay from that checkout matched the archived summary.
