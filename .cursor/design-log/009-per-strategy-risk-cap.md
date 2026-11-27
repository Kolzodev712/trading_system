# Per Strategy Risk Cap

## Problem / context

Strategies mode wires `RiskManager` with `RiskLimits::default()`, so `total_max_position` is `None`. The executor does not clip a bad `TargetPosition`. `BuyLowSellHigh` only self-limits via its own `max_position`. A new prop strategy can request any qty. We need a live Bybit cap before that strategy exists. See Design Log #008.

## Decision

- Cap is **per strategy**, for **live** Bybit USDT linear (same number on demo while we rehearse).
- Source of truth: `max_position` on that strategy’s `strategies.json` row.
- **Executor enforces it** after `evaluate`: clip `|target|` to that strategy’s cap. Strategy-local checks are optional, not sufficient.
- No bot-wide `RiskLimits.total_max_position` in this slice. Add later if several strategies share one account.
- **Initial phase (lose as little as possible):** `max_position = 0.001` BTC (one clip, same as current `position_size`). At ~77k that is ~$77 notional / ~$15 margin at 5x. Raise only after the first prop strategy is proven.

## Alternatives

- **Global `RiskLimits.total_max_position` only** — Rejected: one number for every strategy; user asked per-strategy.
- **Trust each strategy to self-cap** — Rejected: the last burst showed strategy-local inventory/limits are easy to skip.
- **Cap in USDT notional** — Deferred: qty is what Bybit and `TargetPosition` already use; notional can be a later limit.

## Consequences

- Factory/config must pass `max_position` into the executor keyed by `strategy_id`, not only into `BuyLowSellHigh`.
- A target already above cap (e.g. leftover 0.048 vs cap 0.001) can only flatten or Hold — see Design Log #010.
- Stop-loss on 0.001 BTC is still real money on live; this is a size cap, not a guaranteed max loss.

## Verification

- Unit: `|target| > cap` → approved qty is `±cap`; already-inside cap unchanged; `Hold` unchanged.
- Unit: two strategies with different caps — each clip is independent.
- `BuyLowSellHigh` with `max_position = 0.001` still emits at most that target.
- `cargo test -p trading_engine --lib`; clippy on touched crates.

## Implementation results

Shipped: executor clips `|target|` with `approve_target_qty` after `evaluate`, then optional global `RiskLimits.total_max_position` (still `None`). Caps come from each `strategies.json` `max_position` keyed by strategy id. Empty cap map = not configured (no clip). Unknown id fails closed to `0`. Live `BuyLowSellHigh` cap is `0.001`. Unit: `per_strategy_caps_are_independent_and_unknown_fails_closed`. No bot-wide cap in this slice.

