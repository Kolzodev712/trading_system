# Copy everything below this line into the other agent

---

## Mission

You are an **independent auditor** for offline research candidate **#2: BTCPriceMALongFlatV1**. Your job is to prove (or disprove) that the stored experiment artifacts are internally consistent **without** importing the Rust trading engine or treating its code as ground truth.

**Parity standard:** Same independence as `audit/donchian_daily_v1/reference_check.py` (DonchianDailyV1).

**Preregistered screen (confirm, do not override):** disposition **`reject`** — primary N=28 Cost A is net positive but fails 80% benchmark profit retention and the notional-matched passive benchmark.

---

## Repository

- **Repo:** `trading_system` (GitHub: Kolzodev712/trading_system)
- **Branch:** `master` (commit at or after `eff5025`)
- **Read first in repo:** `audit/btc_price_ma_long_flat_v1/AUDIT_BRIEF.md` (full spec)
- **Scaffold to implement:** `audit/btc_price_ma_long_flat_v1/reference_check.py`
- **Reference math (not proof):** `audit/btc_price_ma_long_flat_v1/sources/`, `audit/btc_price_ma_long_flat_v1/tests/`
- **Simulation precedent:** `audit/donchian_daily_v1/reference_check.py` — reuse or factor shared ledger rebuild

---

## Local data (required — not in git)

The operator will provide **`data/btc_price_ma_long_flat_v1/`** (tarball/sync). If missing, regenerate after cloning:

```bash
# Requires data/donchian_daily/ (main daily JSON + aux/) on disk
cargo run -p runner --bin btc_price_ma_experiment -- download-warmup \
  --output data/btc_price_ma_long_flat_v1/warmup-2022-11-01_2023-01-01.json

cargo run -p runner --bin btc_price_ma_experiment -- run \
  --main-dataset data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json \
  --warmup-dataset data/btc_price_ma_long_flat_v1/warmup-2022-11-01_2023-01-01.json \
  --shared-aux data/donchian_daily/aux \
  --output data/btc_price_ma_long_flat_v1
```

**Do not** use `donchian_daily_replay evaluate` (Donchian-frozen CLI).

---

## What to verify

### In scope

1. **Decisions** — For N ∈ {14, 28, 70}, rebuild every row in `decisions-n{N}.json` from merged daily candles (warmup + main), independent Python only.
2. **Target changes** — Ready rows only; quantities **0** or **0.001** BTC.
3. **Backtest ledgers** — Scenarios **`zero_cost_diagnostic`** and **`cost_a`** only, for:
   - Primary N=28
   - Diagnostic N=14 and N=70 (Cost A)
   - Fixed long 0.001 BTC benchmark  
   Match `summary.json`, `fills.csv`, `funding.csv`, `pnl.csv`, `trades.csv`.
4. **Gates** — Recompute the nine gates in `gates.json`; disposition must match.

### Out of scope

`higher_cost`, `delayed_execution`, 2026 holdout, live/testnet, margin/liquidation. For gates 5–6 you may mark “not independently verified” if you only audit zero-cost + Cost A ledgers.

---

## Frozen constants (use literally)

| Item | Value |
| --- | --- |
| Signal | `log(close) − mean(log(N closes))` including current candle; **long if spread > 0**, else flat |
| N primary / diagnostic | 28 / 14, 70 |
| Qty | 0.001 BTC long; flat = 0 |
| Performance window | start `1674259200000` (2023-01-21T00:00:00Z), end exclusive `1767225600000` (2026-01-01T00:00:00Z) |
| zero_cost_diagnostic | 0 fee bps, 0 execution adj bps, 10 s delay |
| cost_a | 6 fee bps, 2 execution adj bps, 10 s delay |
| Benchmark Cost A net / drawdown | `48.360542973936` / `44.3887151092231` USDT |
| Retention gate | primary net ≥ `38.6884343791488` (80% of benchmark net) |
| Drawdown gate | primary max DD ≤ `31.07210057645617` (70% of benchmark DD) |
| Passive gate | `primary_net > 48.360542973936 × f` where `f = Σ\|pos\|×mark` (primary) / same (fixed-long), from Cost A pnl samples |

**Decision JSON quirks:** `entry_high` = geometric mean, `entry_low` = log-spread; `exit_high`/`exit_low` always null.

**Warmup file:** `data/btc_price_ma_long_flat_v1/warmup-2022-11-01_2023-01-01.json`  
**Main daily:** `data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json`  
**Market aux:** `data/btc_price_ma_long_flat_v1/aux/`

---

## Sanity anchors (stored run — your rebuild should match)

| Metric | Value |
| --- | --- |
| N=28 entries / exits | 55 / 55; final flat |
| N=28 Cost A net | `20.6120701293059` USDT |
| N=28 Cost A max drawdown | `30.6689062419564` USDT |
| N=28 zero-cost net | `26.4553879050099` USDT |
| Fixed long Cost A net / DD | `48.360542973936` / `44.3887151092231` |
| N=14 Cost A net | `20.7799676409438` |
| N=70 Cost A net | `42.2272621972557` |
| Gates disposition | `reject` (fails retention + passive; passes net positive, DD cap, higher/delayed summaries, ≥20 trips, diagnostic positives) |

---

## Deliverables

1. Complete `audit/btc_price_ma_long_flat_v1/reference_check.py` so from repo root:

   ```bash
   python3 -m unittest discover -s audit/btc_price_ma_long_flat_v1/tests -v
   python3 audit/btc_price_ma_long_flat_v1/reference_check.py
   ```

   **Exit code 0** only when economic discrepancies are zero for audited scenarios (reason-string wording differences alone are OK if disclosed).

2. Write under **`data/btc_price_ma_long_flat_v1/audit/zero_cost_and_cost_a/`**:
   - `audit_report.md` — § decision correctness, § simulation/accounting, § unverified, § screen result
   - `discrepancies.csv` — all mismatches (empty if clean)

3. **Do not** change Rust to force a match. Report discrepancies honestly.

---

## Suggested order

1. Pass reference unit tests.  
2. Decision rebuild → diff `decisions-n28.json`, then 14 and 70.  
3. Share Donchian simulation core → reconcile zero-cost + Cost A ledgers.  
4. Recompute gates → compare `gates.json`.  
5. Write report.

---

## Success definition

Independent decisions + zero-cost/Cost A ledgers match stored artifacts for primary, diagnostics, and fixed-long; gates/disposition confirmed; limitations documented like the Donchian audit report.

---

# End of handoff
