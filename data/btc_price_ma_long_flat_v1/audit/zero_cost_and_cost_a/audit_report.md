# BTCPriceMALongFlatV1 independent audit

Commit audited: `master` at/after `2cd8211f89aa543f504c13ef9a84051e350b7d62`.

This audit does **not** treat the Rust strategy implementation as ground truth.
Signal decisions were independently rebuilt from the stored warmup + main daily candles.
Execution/accounting was independently reconstructed using the already independent
DonchianDailyV1 Python ledger logic permitted by `AUDIT_BRIEF.md`.

## Decision correctness

| N | Decision rows | Entries | Exits | Final model direction | Economic discrepancies |
| ---: | ---: | ---: | ---: | --- | ---: |
| 14 | 1090 | 79 | 79 | flat | 0 |
| 28 | 1104 | 55 | 55 | flat | 0 |
| 70 | 1146 | 23 | 23 | flat | 0 |

All recorded directions, target kinds, target quantities, timestamps, close values,
readiness states, and strategy identity fields matched the independently rebuilt
decision streams.

The geometric-mean/log-spread diagnostics use transcendental f64/libm math. An
independent runtime can differ in the final floating-point display digits without
changing the spread sign, direction, or target. One N=70 geometric-mean display
comparison was observed at roughly 1e-11 USDT scale during the audit; this is
non-economic and below the audit tolerance. No target changed because of it.

## Simulation / accounting

The following ledgers were independently reconstructed and compared row-by-row
against the stored `fills.csv`, `funding.csv`, `trades.csv`, `pnl.csv`,
and `summary.json` artifacts.

| Book / scenario | Net PnL (USDT) | Max drawdown (USDT) | Fills | Completed trades | End position |
| --- | ---: | ---: | ---: | ---: | ---: |
| N=28 zero_cost_diagnostic | 26.4553879050099 | 29.4494514670964 | 110 | 55 | 0 |
| N=28 Cost A | 20.6120701293059 | 30.6689062419564 | 110 | 55 | 0 |
| N=14 Cost A | 20.7799676409438 | 35.2949480788581 | 157 | 78 | 0.001 |
| N=70 Cost A | 42.2272621972557 | 31.5790661923904 | 46 | 23 | 0 |
| Fixed +0.001 BTC Cost A | 48.360542973936 | 44.3887151092231 | 1 | 0 | 0.001 |

For N=14, the final model exit is timestamped at the performance cutoff, so its
simulated execution falls outside the window and remains pending. The account
therefore ends with +0.001 BTC even though the model direction is flat. No
out-of-window fill was invented.

Primary N=28 Cost A reconstructs exactly:

- gross price PnL: **37.07496940 USDT**
- fees: **4.382487175704 USDT**
- funding: **-12.0804120949901 USDT**
- net PnL: **20.6120701293059 USDT**
- max drawdown: **30.6689062419564 USDT**
- completed trades: **55**

The fixed-long Cost A comparator independently reconstructs to:

- net PnL: **48.360542973936 USDT**
- max drawdown: **44.3887151092231 USDT**

## Notional-matched passive gate

Using the Cost A PnL samples:

- primary notional exposure sum: **932903.30905**
- fixed-long notional exposure sum: **1712235.90266**
- exposure ratio `f`: **0.5448450809848760235550660848...**
- scaled passive Cost A net: **26.34900395310673709601397758... USDT**
- primary Cost A net: **20.6120701293059 USDT**

Therefore the candidate **does not** beat the notional-matched passive control.

## Development screen

| Gate | Result | Verification |
| --- | --- | --- |
| cost_a_net_positive | PASS | independently audited Cost A ledger |
| cost_a_profit_retention_80pct | **FAIL** | 20.6120701293059 < 38.6884343791488 |
| cost_a_drawdown_70pct_of_benchmark | PASS | 30.6689062419564 <= 31.07210057645617 |
| beats_notional_matched_passive | **FAIL** | 20.6120701293059 < 26.3490039531067... |
| higher_cost_net_positive | PASS | stored summary only; ledger not independently audited |
| delayed_execution_net_positive | PASS | stored summary only; ledger not independently audited |
| completed_round_trips_at_least_20 | PASS | 55 completed primary trades |
| diagnostic_14_cost_a_positive | PASS | independently audited N=14 Cost A ledger |
| diagnostic_70_cost_a_positive | PASS | independently audited N=70 Cost A ledger |

## Unverified / out of scope

- `higher_cost` and `delayed_execution` ledgers were not independently reconstructed.
  Their stored summary signs are used only for gates 5–6, as allowed by the audit brief.
- The 2026 holdout was not opened.
- Live/testnet execution, liquidity, partial fills, margin, and liquidation were not verified.

## Screen result

**Disposition: reject.**

The preregistered result is confirmed. The N=28 primary is profitable under Cost A,
but it fails both the 80% benchmark-profit-retention gate and the notional-matched
passive gate. It is therefore not eligible for holdout review under the frozen
candidate #2 specification.
