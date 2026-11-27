# Btc Liquidation Shock V1

## Problem / context

Candidate #3's first experiment is a pre-registered BTCUSDT linear hourly shock-reversal rule. The economic story is temporary overshoot after a large one-hour move. The measured input is that return, not a liquidation print. Candidate #1 lost before costs were decisive, and Candidate #2 failed its exposure-matched passive comparison. The 2026 holdout stays sealed.

The committed BTC linear set can mark and fund the existing research window. It cannot run this rule: there is no hourly trade-price OHLCV, and `trade-1m` is a sparse set of daily decision windows.

## Decision

Freeze `BTCLiquidationShockV1` and evaluate only the primary 3σ / 6-hour specification before any robustness cell.

- Performance window: `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)`.
- Signal: trade-candle log return versus the sample standard deviation (divisor 167) of the prior 168 returns. `r <= -3σ` targets `+0.001` BTC; `r >= +3σ` targets `-0.001` BTC. No overlap. Exit at the close of the sixth later hour, and do not open a new position on that same hour.
- A missing hour, execution minute, mark, or funding settlement fails the run.
- Scenarios stay the existing synthetic set: zero-cost, Cost A (`6` bps + `2` bps + `10` s), higher-cost, and `300` s delay. These are not claimed venue fees.
- Placebo: same event times, opposite sign. Passive BTC is reported, not a kill gate.
- Promotion requires at least 20 completed non-overlapping trades, Cost A / higher-cost / delayed nets all `> 0`, and Cost A net strictly above the placebo. Also reject if removing the largest winning trade leaves Cost A net `<= 0`, or if two of {2023, 2024, 2025} have entry-year Cost A net `< 0`.
- Below 20 trades, do not open the nine-cell matrix. After promotion, a neighbor supports the claim only with at least 20 trades, Cost A net `> 0`, and a win over its own placebo. Require at least 3 of the 4 single-knob neighbors. Do not promote another cell.
- Pure rule lives in `strategy_core`. Research owns the dataset, replay, and the existing fill/funding/PnL simulator. No runner, composition, Shadow, k-paper, UI, or Demo work in this experiment.
- Existing mark and funding files are reused only if their hashes and grids pass. Hourly trade candles, then decision-timed execution minutes, are a new versioned dataset. Do not patch Candidate #1 or #2 files.

## Alternatives

- Research-only script with the rule outside `strategy_core`: a survivor would need the math rewritten.
- Full Candidate #2 operational path before the first result: spends runner and composition work on a rule the kill test may reject.
- Full three-year minute tape: unnecessary once decision times are frozen; the simulator only fills specific post-decision minutes.

## Consequences

The coverage gate on the current files fails before any PnL. A dirty git worktree cannot produce the performance disposition. Robustness and an independent audit happen only after the primary promotion gate. Volatility terciles are descriptive and are not a filter.

## Verification

- Pure-model tests for the shock sign, the inclusive 3σ boundary, overlap, the six-hour exit, and the placebo flip.
- Dataset tests that a missing hour or a missing execution minute fails closed.
- Coverage command writes a fail report for the committed aux set.
- Performance command records spec hashes, dataset hashes, git revision, ledgers, gates, and a summary. Disposition is reject or provisional pass. It is not a validated development pass until a later independent audit.

## Implementation results

Shipped the frozen spec, the pure `strategy_core` rule, and the research path. No runner, composition, Shadow, k-paper, UI, or Demo work.

Coverage gate on the committed aux set failed closed, as required: mark-60 and funding passed their frozen hashes and grids; hourly trade OHLCV was absent; the existing `trade-1m` file is the known sparse daily-window set (1,560 opens). Report: `data/btc_liquidation_shock_v1/coverage_gate.json`.

New versioned dataset, not a patch of Candidate #1 or #2:

- Hourly trade candles: 25,993, `sha256:806a91c0afedee338bd2435a753e9a862e10279c9a9d12e969aabcfa839e415a`
- Decision-timed execution minutes: 4,998 opens, no gaps, `sha256:3a1941cd9facb31fed335172660c9b814521edef2c8b03bef1dc07fea2e84c4c`
- Reversal decisions: 416 entries and 416 exits. Primary config hash `sha256:5df3141260c7d66b8685dcd7910883cd7e29b8a26b468e6cdcd8c68d4ebaca34`

The performance command ran on revision `8e20b5f53b83101c2dba05e823d9ad69d91ccd9e` with a clean worktree. Disposition: **reject**. Cost A reversal net is -57.35 USDT over 416 trades. Zero-cost reversal net is -13.97, so the loss is present before fees. The continuation placebo Cost A net is -29.41, so reversal does not beat continuation. 2023, 2024, and 2025 entry-year Cost A nets are all negative. The robustness neighborhood was not opened. No runner or composition work.
