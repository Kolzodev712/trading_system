# BTCLiquidationShockV1

Candidate version 1. This file is the human-readable preregistration. The machine-readable twin is `experiment.json` in this directory. After the first performance run, this version is immutable. A changed trading rule is a new version.

## Hypothesis

Large short-horizon BTC perpetual moves partly reflect forced liquidation, stop cascades, and temporary liquidity withdrawal. Once that forced flow exhausts, some of the move reverses over the following hours.

The proposed edge is temporary price overshoot. The model does not observe liquidation prints, open interest, or RSI. The measured object is a completed-hour log-return shock.

## Instrument and window

- Venue: Bybit mainnet, USDT linear perpetual, symbol `BTCUSDT`.
- Signal timeframe: completed 1-hour trade-price candles. A candle's identity is its open time. Its close time is one hour later.
- Performance window: `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)`. This is the existing research window, not calendar 1 January 2023.
- An hour is eligible when its close time is inside that window and the preceding 170 trade closes exist on an exact one-hour grid. Warmup closes may begin before the window. They never create a position or PnL.
- 2026 is not loaded.

## Primary rule

For each eligible hour `t`, with trade closes `C`:

1. `r_t = ln(C_t / C_{t-1})`.
2. Rolling volatility is the sample standard deviation of the preceding 168 log returns, excluding `r_t`, divisor 167.
3. A zero or non-finite sigma fails the run. A missing, duplicate, or off-grid hour fails the run. The window is not shortened to skip a hole.
4. If `r_t <= -3σ`, target `+0.001` BTC. If `r_t >= +3σ`, target `-0.001` BTC. Otherwise the target stays flat.
5. While a position is open, ignore new shocks.
6. At the close of the sixth completed hour after the signal close, target returns to `0`. That exit hour does not also open a new position.
7. No stop, take-profit, volume filter, funding filter, or volatility filter.

Comparison uses f64 for the log return, the standard deviation, and the threshold. Position quantity stays a decimal.

## Execution and accounting

Enter and exit on the first one-minute trade open strictly after the decision close plus the scenario delay. The fill quantity is the full target change. Fee and adverse adjustment are the scenario assumptions below, applied by the existing research simulator. They are not a claim about the current Bybit fee tier.

Scenarios:

| Name | Fee | Adverse adjustment | Delay |
| --- | --- | --- | --- |
| zero_cost_diagnostic | 0 bps | 0 bps | 10 s |
| cost_a | 6 bps | 2 bps | 10 s |
| higher_cost | 10 bps | 10 bps | 10 s |
| delayed_execution | 6 bps | 2 bps | 300 s |

A missing execution minute, a missing hourly mark, or a missing funding settlement inside the performance window fails the run. Nothing is interpolated or forward-filled.

If a hold would finish after the window, the exit is recorded as pending at the window end and no fill is invented.

## Placebo and passive

The continuation placebo uses the same threshold, hold, size, and event times, with the target sign flipped: a down shock is short and an up shock is long. It is not a second hypothesis.

The passive result is a notional-exposure-scaled always-on long of `0.001` BTC, the same scaling Candidate #2 used. It is a report row, not a kill gate. Candidate #2's Cost A net is cited only as full-period context.

## Promotion and rejects

Primary specification only (`3σ`, 6 hours, reversal):

- At least 20 completed, non-overlapping round trips. Fewer than 20 rejects Experiment 1 and does not open the robustness matrix.
- Cost A net `> 0`, higher-cost net `> 0`, and delayed net `> 0`.
- Cost A net strictly greater than the placebo's Cost A net.
- Reject if Cost A net minus the single largest winning trade is `<= 0`.
- Reject if two of the entry years 2023, 2024, and 2025 have Cost A net `< 0`. The year is the year of the entry decision time. A trade's whole net is attributed to that year.

Zero-cost is diagnostic. It is not a gate.

## Robustness

Runs only after the promotion checks above pass. Nine cells: thresholds `2.5σ`, `3.0σ`, `3.5σ` and holds `3`, `6`, `12` completed hours. Neighbors are scored under Cost A and their own continuation placebo only.

A neighbor supports the claim when it has at least 20 completed trades, Cost A net `> 0`, and Cost A net greater than its placebo. Otherwise it does not support the claim.

Reject unless at least 3 of these 4 single-knob neighbors support the claim: `2.5σ/6h`, `3.5σ/6h`, `3σ/3h`, `3σ/12h`. No cell replaces the primary specification.

## Descriptive split

High, medium, and low volatility are terciles of the trailing sigma at primary entries, cut on this development window after the run. They are reported with the cut values. They are not a filter and not a gate.

## What this experiment does not include

Composition, the generic runner, Shadow, k-paper, the UI, and Demo. An independent audit waits until the primary promotion gate and the robustness gate both pass.
