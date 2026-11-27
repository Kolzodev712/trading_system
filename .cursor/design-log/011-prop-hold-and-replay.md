# Prop Hold And Replay

## Problem / context

The intent path, 0.001 cap, and flat demo book are in place (Design Logs #008–#010). There is no edge yet. Shipping a guessed live rule would encode noise. We need a live slot that cannot open risk, and an offline replay so a later rule can be tested before it trades.

## Decision

- **Live:** new `PropHold` strategy. Flat → `Hold`. Non-zero current qty → `TargetPosition` 0 (flatten only). Factory `strategy_type`: `PropHold`. Switch live `strategies.json` to it. Cap stays 0.001.
- **Offline:** candle replay over the same `evaluate` → intent → delta path. Public Bybit linear klines (no keys) or a JSON file. Not a full research platform; no live indicators yet.
- No live signal. Trading stays off by default.

## Alternatives

- **Keep BuyLowSellHigh live** — Rejected: accidental enable can open size.
- **Guess a textbook rule now** — Rejected: no edge; demo luck is not proof.
- **Build indicator context first** — Deferred until a rule needs specific features.

## Consequences

- Live config still has BLSH-shaped fields (`buy_threshold_percent`, etc.); `PropHold` ignores them.
- Replay uses a synthetic one-tick book from each candle close. That is good enough to reject junk rules, not to prove microstructure.
- `grpc_ctl enable` must target `PropHold`, not the old name.

## Verification

- Unit: flat account → `Hold`; leftover qty → `TargetPosition` 0.
- Replay: `PropHold` on flat history → 0 trades; leftover start → one reduce to 0.
- `build_strategy("PropHold")` succeeds; unknown type still errors.
- `cargo test -p trading_engine --lib`; clippy on touched crates.

## Implementation results

Shipped: `PropHold` (flat → `Hold`, leftover → `TargetPosition` 0). Factory `PropHold` / `prop_hold`. Live `strategies.json` switched to it (cap still 0.001). Replay module + `cargo run -p runner --bin prop_replay` (file or public Bybit linear klines). `grpc_ctl` defaults to `PropHold`. Removed unused `backtest.rs` stub.

Verified: 77 `trading_engine` lib tests; replay CLI flat = 0 trades; leftover `0.048` = one sell to 0; Bybit 24×60m klines = 24 holds, 0 trades. Clippy clean on `trading_engine` lib and `prop_replay --no-deps`.
