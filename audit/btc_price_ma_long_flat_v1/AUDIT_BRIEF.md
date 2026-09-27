# Independent audit brief: BTCPriceMALongFlatV1 (candidate #2)

**Audience:** An agent or reviewer who must **not** treat the Rust trading engine as ground truth. Rebuild decisions and (where scoped) accounting from stored inputs, then compare to committed run artifacts.

**Parity target:** Same independence standard as `audit/donchian_daily_v1/reference_check.py` for DonchianDailyV1.

**Preregistered screen outcome (to confirm, not to override):** `reject` — primary N=28 Cost A net positive but fails 80% benchmark profit retention and notional-matched passive benchmark.

---

## 1. Scope

### In scope

| Layer | What to verify |
| --- | --- |
| **Signal / decisions** | For N ∈ {14, 28, 70}, rebuild every row in `decisions-n{N}.json` from the merged daily candle series (warmup + main), without calling Rust `on_candle`. |
| **Target changes** | Derive the same target-change stream the simulator uses (ready rows only; qty 0 or 0.001). |
| **Backtest (subset)** | **Zero-trading-cost diagnostic** and **Cost A** for: primary N=28, diagnostics N=14 and N=70, and **fixed long 0.001 BTC** benchmark — match `summary.json`, `fills.csv`, `funding.csv`, `pnl.csv`, `trades.csv` where present. |
| **Development screen** | Recompute the nine gates in `gates.json` from independently simulated primary/diagnostic/benchmark Cost A summaries (or from reconciled ledgers). Disposition must match. |

### Out of scope (explicit)

- `higher_cost` and `delayed_execution` scenarios (same policy as Donchian audit).
- 2026 holdout (sealed; not opened for this run).
- Live/testnet execution, liquidity, partial fills, margin, liquidation.
- Re-running `donchian_daily_replay evaluate` (Donchian-frozen CLI).

---

## 2. Frozen constants

Copy these literally; do not read them from Rust at audit time.

| Constant | Value |
| --- | --- |
| Strategy id | `BTCPriceMALongFlatV1` |
| Strategy version | `1` |
| Primary lookback N | `28` |
| Diagnostic lookbacks | `14`, `70` |
| Position size | `0.001` BTC (long only; flat = 0) |
| Performance start (inclusive) | `1674259200000` ms — 2023-01-21T00:00:00Z |
| Performance end (exclusive) | `1767225600000` ms — 2026-01-01T00:00:00Z |
| Signal | `log(close) − mean(log(closes))` over **N** completed daily closes **including** the current candle; long iff spread **> 0**, else flat |
| Geometric mean (equivalent) | `(∏ closes)^(1/N)`; compare via log form for stability |
| Initial model/account | Flat; no simulated trades before `performance_start` |
| Warmup dataset | `data/btc_price_ma_long_flat_v1/warmup-2022-11-01_2023-01-01.json` (ends where main starts) |
| Main daily dataset | `data/donchian_daily/bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json` |
| Shared aux (copy in run tree) | `data/btc_price_ma_long_flat_v1/aux/` — trade-1m, funding, mark-60 (hashes in run metadata / aux JSON) |

**Cost scenarios (same frozen simulator as Donchian development experiment):**

- `zero_cost_diagnostic`: fee 0 bps, execution adjustment 0 bps, 10 s decision delay
- `cost_a`: fee 6 bps, execution adjustment 2 bps, 10 s decision delay

**Benchmark constants for gates (fixed at preregistration):**

- Benchmark Cost A net: `48.360542973936` USDT  
- Benchmark Cost A max drawdown: `44.3887151092231` USDT  
- Min profit fraction: `0.8` → required primary net ≥ `38.6884343791488`  
- Max drawdown fraction: `0.7` → primary max drawdown ≤ `31.07210057645617`  
- Min completed round trips: `20`  

**Notional-matched passive gate:**  
`f = sum(|position| × mark) over primary Cost A pnl samples / same sum for fixed-long Cost A`  
Pass iff `primary_net > benchmark_net × f`.

---

## 3. Input artifacts (repository-relative)

Assume repo root `trading_system/`. Run output lives under `data/btc_price_ma_long_flat_v1/` (gitignored; must exist locally from experiment).

```
data/btc_price_ma_long_flat_v1/
  decisions-n14.json
  decisions-n28.json
  decisions-n70.json
  gates.json
  experiment_report.md
  aux/trade-1m.json
  aux/… (funding, hourly marks — same layout as donchian_daily aux)
  backtest/
    primary_n28/n28/{zero_cost_diagnostic,cost_a}/…
    diagnostic_n14/n14/cost_a/…
    diagnostic_n70/n70/cost_a/…
    fixed_long_0.001_btc/fixed_long_0.001_btc/cost_a/…
```

**Recorded headline numbers (sanity anchors):**

- Primary N=28 Cost A: net `20.6120701293059`, max drawdown `30.6689062419564`, 55 completed trades  
- Primary entries/exits: 55 / 55; final direction flat  
- Fixed long Cost A net: ~`48.360542973936`, drawdown ~`44.3887151092231`  
- Gates disposition: `reject` (see `gates.json`)

---

## 4. Decision log schema notes

Decision rows reuse the Donchian offline JSON shape. For MA, channel fields are **repurposed**:

- `entry_high` → geometric mean of the N closes (decimal string), when ready  
- `entry_low` → log-spread (decimal string), when ready  
- `exit_high` / `exit_low` → always null  
- `readiness`: `not_ready` for the first N rows of the merged series window; then `ready`  
- `target.kind`: `not_ready` | `no_change` | `quantity`  
- Entry: ready, previous `flat`, direction `long`, target qty `0.001`  
- Exit: ready, previous `long`, direction `flat`, target qty `0`  

Config hash payload (for cross-check only): strategy id/version, lookback N, position qty, `allow_short: false`, signal string `log_close_minus_geometric_mean_of_N_including_current`.

---

## 5. Reconstruction algorithm (decisions)

1. Load warmup + main datasets; merge on contiguous daily candles (same rules as offline `merge_datasets`: no gaps, no conflicting duplicates).  
2. Build the evaluation window series from `actual_start` through `evaluation_end_exclusive` (offline uses completed candles + lookback windowing — mirror `completed_series` / `window_series` behavior from Donchian offline, or read decision timestamps only from stored log for cross-check).  
3. Maintain `requested_qty` ∈ {0, 0.001}.  
4. For each candle index `i` with window `closes[i-N+1..=i]` (once `i ≥ N-1` in the replay index semantics used in Rust):  
   - Compute log-spread in **f64** (Rust uses `Decimal` → f64 → `ln` per close).  
   - If spread ≤ 0 → target 0; if spread > 0 → target 0.001.  
   - Emit direction `long` iff target > 0 else `flat`.  
5. Compare **every** stored field listed in §4 for all three N; count independent entries/exits with the same predicates as Rust `is_entry` / `is_exit`.  
6. Expected independent counts for the shipped run: N=28 → 55 entries, 55 exits.

**Reference implementation (for development, not proof):** `audit/btc_price_ma_long_flat_v1/sources/model_reference.py` and `tests/test_model_reference.py`.

---

## 6. Reconstruction algorithm (simulation)

The MA experiment calls the **same** frozen backtest as Donchian (`research_scenarios`, `simulate`, funding, hourly marks, adverse-adjusted minute opens). For execution/accounting:

- **Preferred:** Factor shared simulation code out of `audit/donchian_daily_v1/reference_check.py`, or import/reuse its ledger rebuild for a given `target_changes` stream and market inputs.  
- **Inputs:** Target changes from independently rebuilt decisions (N=28 primary + diagnostics + fixed-long schedule from performance start).  
- **Market:** `data/btc_price_ma_long_flat_v1/aux/trade-1m.json` plus funding/mark files copied from `data/donchian_daily/aux` (verify sha256 in aux metadata if present).

Match Donchian audit tolerances: exact decimal equality on stored ledger strings unless a documented formula ambiguity exists (disclose, do not “fix” the engine).

---

## 7. Gate recomputation

From reconciled Cost A simulations:

1. `cost_a_net_positive`  
2. `cost_a_profit_retention_80pct`  
3. `cost_a_drawdown_70pct_of_benchmark`  
4. `beats_notional_matched_passive`  
5. `higher_cost_net_positive` — **may** trust stored summary only (out of full ledger audit scope) OR mark “not independently verified”  
6. `delayed_execution_net_positive` — same  
7. `completed_round_trips_at_least_20`  
8. `diagnostic_14_cost_a_positive`  
9. `diagnostic_70_cost_a_positive`  

Reference formulas: `sources/gates_reference.py`, tests in `tests/test_gates_reference.py`.

---

## 8. Deliverables

1. **`reference_check.py`** (this directory) — runnable from repo root:  
   `python3 audit/btc_price_ma_long_flat_v1/reference_check.py`  
2. **Output directory:** `data/btc_price_ma_long_flat_v1/audit/zero_cost_and_cost_a/` (create if missing) containing at minimum:  
   - `audit_report.md` — conclusions § decision correctness, § simulation/accounting, § unverified, § screen result  
   - `discrepancies.csv` — all mismatches (empty if clean)  
   - Optional side-by-side CSVs for decisions/fills (mirror Donchian audit style)  
3. **Exit code:** 0 only if economic discrepancies (excluding reason-string wording) are zero for audited scenarios.

---

## 9. Acceptance criteria

- Independent decision rebuild matches all three decision logs (economic fields).  
- Zero-cost + Cost A ledgers match for primary N=28, N=14, N=70 Cost A, and fixed-long Cost A.  
- Recomputed gates match `gates.json` for all gates derived from audited ledgers.  
- Report states clearly that higher_cost / delayed / holdout / live execution were not verified.  
- Do **not** modify Rust sources to force a match; file discrepancies.

---

## 10. Suggested work order

1. Run `python3 -m unittest discover -s audit/btc_price_ma_long_flat_v1/tests -v` (reference model/gates).  
2. Implement decision rebuild; diff against `decisions-n28.json` first.  
3. Port or share simulation core from Donchian `reference_check.py`.  
4. Reconcile primary + benchmark Cost A; then diagnostics.  
5. Write `audit_report.md` and gate summary.

---

## 11. Related engine locations (for humans only; auditor should not depend on these)

Implementation lives under `src/trading_engine/.../btc_price_ma_long_flat_v1/` and runner bin `btc_price_ma_experiment`. The auditor brief intentionally keeps proof in Python + stored JSON.
