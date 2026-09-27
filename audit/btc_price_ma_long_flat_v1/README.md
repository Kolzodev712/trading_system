# BTCPriceMALongFlatV1 — independent audit workspace

Handoff for a separate reviewer agent. **Start with [AUDIT_BRIEF.md](./AUDIT_BRIEF.md).**

| Path | Role |
| --- | --- |
| `AUDIT_BRIEF.md` | Scope, constants, algorithms, deliverables |
| `reference_check.py` | Entry point for the full audit (implement ledger compare per brief) |
| `sources/` | Reference implementations kept **outside** the Rust crate |
| `tests/` | Executable checks for reference math (sources of work) |

```bash
# Reference unit checks (no data/ required)
python3 -m unittest discover -s audit/btc_price_ma_long_flat_v1/tests -v

# Full audit (requires local experiment output under data/btc_price_ma_long_flat_v1/)
python3 audit/btc_price_ma_long_flat_v1/reference_check.py
```

Donchian precedent: `audit/donchian_daily_v1/reference_check.py`.
