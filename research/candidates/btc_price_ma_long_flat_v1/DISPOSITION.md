# BTCPriceMALongFlatV1 research disposition

The **historical alpha screen** is `REJECTED` + `ARCHIVED`.

This is a **legacy** record. It was not preregistered on `master` under the
research-record process. Do not read this reject as an instruction to delete
the live model: `strategy_core::btc_price_ma_long_flat_v1` and
`btc_price_ma_app` stay in the workspace while Demo/Paper observation of
Candidate #2 continues.

- Window: `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)` end exclusive
- Primary N=28 Cost A net: `20.6120701293059`
- Primary N=28 zero-cost net: `26.4553879050099`
- Higher-cost N=28 net: `11.8471204408099`
- Delayed N=28 net: `20.5469761215179`
- Completed trades (primary): 55
- N=14 Cost A: `20.7799676409438`
- N=70 Cost A: `42.2272621972557`
- Fixed-long Cost A: `48.3605429739360`
- Failed gates: `cost_a_profit_retention_80pct`, `beats_notional_matched_passive`
- Independent audit (`3f3f943` / `08dab4b`): 0 economic decision discrepancies
  on N=14/28/70; Cost A ledgers matched
- 2026 holdout: `UNOPENED` (not validation)

## Provenance

Ledgers first tracked in `2cd8211f89aa543f504c13ef9a84051e350b7d62`. The
audit names that commit as the audited snapshot. Summaries do not record a
dirty/clean scoring SHA. Daily series dataset hash
`sha256:bb642e2c001a0df039b15fbad7a7032cab61ae41f29eef785823facb55265236`.

Command: `cargo run -p research_cli -- btc-price-ma run`. Reconstruct
historical ledgers from the local artifact after bulky `data/` files leave
HEAD.
