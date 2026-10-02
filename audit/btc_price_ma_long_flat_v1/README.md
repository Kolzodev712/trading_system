# BTCPriceMALongFlatV1 — independent audit workspace

Handoff for a separate reviewer agent. **Start with [AUDIT_BRIEF.md](./AUDIT_BRIEF.md).**

| Path | Role |
| --- | --- |
| `AUDIT_BRIEF.md` | Scope, constants, algorithms, deliverables |
| `reference_check.py` | Full audit after restoring archive ledgers that are no longer on HEAD |
| `sources/` | Reference implementations kept **outside** the Rust crate |
| `tests/` | Executable checks for reference math (sources of work) |

Decision logs, `gates.json`, warmup, and aux still live under
`data/btc_price_ma_long_flat_v1/` on HEAD. Generated backtest and audit
ledgers do **not**. Restore artifact `btc_price_ma_long_flat_v1-v1-primary`
from the local store (second-device copy) before a full ledger compare.

```bash
# Reference unit checks (no restored ledgers required)
python3 -m unittest discover -s audit/btc_price_ma_long_flat_v1/tests -v

# Full audit: restore the archive first, then
python3 audit/btc_price_ma_long_flat_v1/reference_check.py
```

See `research/candidates/btc_price_ma_long_flat_v1/` for the compact record
and restore notes. Donchian precedent: `audit/donchian_daily_v1/reference_check.py`.
