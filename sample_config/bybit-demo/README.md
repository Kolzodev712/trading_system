# Bybit Demo — Candidate #2 (real signal)

Dedicated config for Bybit **Demo** (`demo: true`, `testnet: false`), USDT linear Futures, BTC/USDT only, `execution_mode=exchange`, N=28, `max_entry_age_secs=86400`.

`sample_config/bybit/` is a placeholder Spot ETH/SOL bot. Do not use it for this run.

Launch with the composition binary and the long-running runbook (not `scripts/inc10_bybit_demo.sh`, which measures five minutes then flattens):

See [docs/BYBIT_DEMO_RUNBOOK.md](../../docs/BYBIT_DEMO_RUNBOOK.md).

Replace the Demo API key placeholders before launch. Journal, ledger, snapshot, and logs live under `runtime-data/bybit-demo/` (gitignored). Candidate #2 remains an operational baseline, not validated alpha.
