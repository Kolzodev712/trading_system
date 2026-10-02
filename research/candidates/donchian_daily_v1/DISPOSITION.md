# DonchianDailyV1 disposition

Final for candidate version 1. Status: `REJECTED` + `ARCHIVED`.

This is a **legacy** record. It was not preregistered on `master` under the
research-record process introduced for BTCLiquidationShockV1.

- Window: `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)` end exclusive
- Cost A Donchian net: `-41.4028767858150`
- Zero-cost Donchian net: `-38.2577787114470`
- Higher-cost Donchian net: `-46.1205443978470`
- Delayed Donchian net: `-43.0533657038030`
- Cost A fixed-long comparator: `48.3605429739360` (report row, not a pass)
- Independent audit of zero-cost and Cost A: economic decision mismatches 0;
  the economic screen still fails because Cost A net is negative
- Robustness matrix: not a later-process gate; not opened under that name
- 2026 holdout: `UNOPENED` (not validation)
- Shadow / Paper / Demo of this research model: not opened

The live factory type `Donchian` remains selectable and is a different
implementation. `DonchianDailyV1` is not a live strategy.

## Provenance

Stored ledgers were first tracked in Git as
`2cd8211f89aa543f504c13ef9a84051e350b7d62`
("Track Donchian and BTCPriceMALongFlatV1 experiment data for independent
audit."). That commit is the ledger snapshot, not a verified scoring run.
Summaries do not record a scoring SHA or a clean/dirty state.
`research_cli` did not exist at `2cd8211`. The contemporaneous command family
was `cargo run -p runner --bin donchian_daily_replay -- evaluate`; that is
not a claim that this exact invocation produced the stored nets. Dataset hash
`sha256:bb642e2c001a0df039b15fbad7a7032cab61ae41f29eef785823facb55265236`,
signal config `sha256:8057b5024b6df11096bc7541e8f470e52794f5e2025503da45beedc0aab33114`.

Reconstruct from the local artifact. Later stored-decision replay through the
shared simulator is a check of the kept ledgers, not a replacement scoring SHA.
