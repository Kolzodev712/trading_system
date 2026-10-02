# DonchianDailyV1

Legacy Candidate #1. This file describes the frozen research rule that was
scored. It is **not** a master preregistration. The spec was never committed
to `master` as `PREREGISTERED` before scoring.

This research model is **not** the live factory strategy named `Donchian`.
The runner still builds `Donchian` for Paper/sample use. `DonchianDailyV1`
is rejected by the live factory.

## Hypothesis

A 20-day / 10-day Donchian channel on completed daily BTCUSDT linear candles
can capture trend after a break of the prior 20-day high or low, and exit on
a break of the prior 10-day opposite extreme.

## Frozen rule (as scored)

- Venue: Bybit mainnet, USDT linear perpetual, `BTCUSDT`, interval `D`.
- Entry lookback 20, exit lookback 10, current candle excluded, strict
  inequalities, shorting enabled, quantity `0.001` BTC.
- No same-candle reversal or resize.
- Performance window `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)` (UTC,
  end exclusive). The 20 candles immediately before the window are warm-up
  and produce no target.
- Execution: first one-minute trade open after the decision close plus the
  scenario delay. Research simulator assumptions, not a fee-tier quote.

Scenarios: `zero_cost_diagnostic` (0/0/10s), `cost_a` (6bps+2bps/10s),
`higher_cost` (10bps+10bps/10s), `delayed_execution` (6bps+2bps/300s).
Independent audit covered zero-cost and Cost A only.
