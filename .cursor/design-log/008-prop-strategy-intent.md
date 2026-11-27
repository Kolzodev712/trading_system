# Prop strategy intent and Bybit linear positions

## Problem / context

The end goal is a prop bot on Bybit (first) and Binance (later). The existing CEX stack already has market data, exchange I/O, risk, and a strategy runner — but strategies emit `OrderRequest` / cancel IDs, `RiskManager` is wired only on the **levels** path, and position authority is spot-shaped (base-balance heuristic). Target-vs-current execution is therefore unsafe for USDT linear perps.

Live strategy code is **not** `src/strategy_engine` (unwired scaffold / Proposal 02 Python IR). It lives in `src/trading_engine/src/cex/strategies/` and `src/runner/src/cex/strategies.rs`.

## Decision

Evolve the **existing CEX strategy path** into one contract for old and new strategies:

```text
MarketData / (later) indicators
        → Strategy.evaluate(ctx) → StrategyIntent
        → RiskManager (may reduce or reject)
        → delta = approved_qty − current_qty
        → AssetService normalize
        → TradingClient
        → ExchangeStateManager (authoritative)
        → TradingStateManager (derived read model)
        → fill feedback to strategy-local state only
```

**Venue:** Bybit USDT linear (`InstrumentType::Futures` → `BybitCategory::Linear`) first. One-way position mode in v1. Binance futures stays `NotSupported` on `TradingClient` until a later client. k-paper is for unit-testing the delta/risk loop, not a futures substitute.

**Intent v1 (small):**

- `StrategyIntent::Hold` — warmup, missing data, or unchanged
- `StrategyIntent::TargetPosition { strategy_id, instrument, qty }` — signed qty; `0` = flat

No runtime `StrategySignal`. No `TargetExposure` yet. Keep `strategy_id` on intent so a portfolio layer can be added later.

**Unified `Strategy` trait (in place, not a parallel path):**

- Manager still owns enable/disable + `TradingControl` override
- `evaluate(ctx) -> StrategyIntent`; `ctx` has market snapshot + **read-only** account snapshot
- `on_execution_report` updates **strategy-local** state only
- Drop trait-level `on_orderbook_cancels` and `get_state` / `get_state_mut` as account truth
- New strategies: `strategies.json` → factory match arm → `StrategyManager` (same as `BuyLowSellHigh`)

**Position authority (perps):** `ExchangeStateManager` signed futures positions are the only input to `target − current`. `TradingStateManager` must read those, not invent spot inventory from base balance. Strategies must not keep a second authoritative position.

**Risk:** always on the strategy path. May reduce or reject. Extend `RiskLimits` over time (per-symbol qty, notional, leverage). v1 at least applies existing limits to the implied target/order.

**Execution v1:** one (or few) orders for the delta. No TWAP/VWAP/post-only yet. Working-order lifecycle belongs to the executor, not the strategy.

**Levels and DEX:** stay separate modes. Do not force them through `StrategyIntent` in this log.

## Invariants

1. Strategy logic MUST NOT know how an order is sent, amended, sliced, or which REST/WS endpoint is used.
2. Strategy MUST NOT own authoritative account position, balances, or open orders.
3. Risk can always reduce or reject intent. Intent is a request, not a command.
4. Execution MUST NOT change the alpha decision (direction / desired target), only how to reach the approved target.
5. `TradingControl == false` overrides any enabled strategy: no new executable orders; cancel working orders (existing behavior).
6. A disabled or errored strategy MUST emit `Hold` (or not be evaluated).
7. Missing warmup / incomplete data → `Hold`. Do not fabricate features or trade from empty history.
8. One strategy per instrument in v1. Conflicting multi-strategy netting is out of scope.
9. Tick/step/min-notional normalization happens only in the execution/validation layer (`AssetService`), not inside strategy alpha.
10. Bybit one-way mode in v1; hedge mode is a later change.

## Alternatives

- **Parallel intent path + keep `OrderRequest` strategies** — Rejected: two contracts; new prop strats would not wire like existing ones.
- **Signal-only output (Long/Short/Neutral)** — Rejected for live: sizing would still live in the strategy or a missing sizer. Target qty is the live contract; signal can be metadata later.
- **Put types in `strategy_engine` / implement Proposal 02** — Rejected: crate is unwired; Python/IR is out of scope.
- **Build Portfolio Manager now** — Rejected until multiple simultaneous strategies share an instrument.
- **Perps + Binance futures in the same slice** — Rejected: Bybit first; Binance waits.
- **Per-strategy tasks / latest-value coalescing now** — Deferred: keep `Arc<RwLock>` + sequential evaluate for v1.

## Consequences

- Breaking change to `Strategy` and `BuyLowSellHigh`; tests and `StrategyManager` / `StrategyExecutor` / runner factory must move together.
- `TradingStateManager` reconciler must branch: futures → ESM positions; spot levels path can keep current heuristic until separately migrated.
- Bybit `TradingClient` must implement `fetch_positions`, collateral, leverage, position mode so bootstrap/reconcile matches WS updates.
- Executor becomes the single place that holds `TradingClient`; strategies lose even the unused constructor param.

## Phased plan

1. **Bybit linear position authority** — REST fetch + reconcile into ESM; TSM uses signed futures qty; one-way + config leverage at bootstrap.
2. **Intent types + trait + factory** — `StrategyIntent` in `trading_core` or `trading_engine`; evolve `Strategy`; factory dispatch.
3. **IntentExecutor** — target − current → risk → normalize → `TradingClient`; honor `TradingControl`.
4. **Migrate `BuyLowSellHigh`** — emit target qty (including flat); drop unused client.
5. **Prove** — unit tests for delta/risk/disable; mock-client integration; Bybit testnet is manual/ops, not a code gate.

## Out of scope

Python DSL / IR, DEX `TradeCommand` unification, levels → intent, feature-engine rewrite, multi-strategy netting, Binance futures, TWAP/VWAP, hedge mode.

## Verification

- Unit: `target − current` (long, short, flatten, already-there, risk-reduced).
- Unit: disabled strategy and `TradingControl=false` produce no executable orders.
- Bybit mapping tests for position/collateral (extend existing REST type tests).
- Integration: mock `TradingClient` + fake ESM positions through the new executor.
- `cargo fmt`, `cargo clippy -D warnings`, targeted `cargo test` for touched crates.
- Manual later: Bybit testnet one-way linear, one instrument.

## Implementation results

- **Bybit linear authority:** `BybitClient` implements `fetch_positions`, `fetch_position`, `fetch_collateral_balance`, `get/set_leverage`, `get/set_position_mode`. REST types: `BybitPositionListParams`, `BybitSetLeverageRequest`, `BybitSwitchModeRequest`, `position_idx`. Optional `BybitConfig.leverage`. Bootstrap sets one-way + leverage (warn-and-continue on failure). ESM bootstrap/periodic reconcile applies REST positions and removes missing pairs.
- **TSM:** `authoritative_position_qty` — futures use signed ESM qty (flat = 0), spot keeps base-balance fallback. Subscriber applies `CexVenueUpdate::Position` in futures mode and ignores trade-derived inventory. `TradingState::set_position` for absolute updates. Runner constructs TSM with `new_with_instrument_type`.
- **Intent contract:** `cex/intent.rs` — `StrategyIntent::{Hold, TargetPosition}`, `position_delta`, `apply_abs_qty_cap`, `intent_to_order_request`. `Strategy::evaluate(ctx)` replaces `on_orderbook` / cancel IDs. Factory `build_strategy` for `BuyLowSellHigh`.
- **Executor:** collects intents, caps abs qty from `RiskLimits.total_max_position`, converts delta to a limit `OrderRequest`, runs `RiskManager::check_pre_trade`, then existing bulk place/amend. `TradingControl` still skips evaluation when disabled.
- **BuyLowSellHigh:** emits target qty (flat on stop/take-profit); unused `TradingClient` constructor arg removed.
- **Verification:** `cargo test -p exchange_engine --lib` (232 ok); `cargo test -p trading_engine --lib` (68 ok, including delta/cap and futures-vs-wallet qty); `cargo check -p runner`; `cargo clippy -p trading_engine -p exchange_engine --all-targets -- -D warnings` clean. Bybit testnet remains a manual ops check.
- **Incidental 1.98 clippy:** `market_data_engine` sort + DEX quote match-if collapse so workspace clippy can run on 1.98.1.
- **Demo bugfix:** `BuyLowSellHigh` stamped `InstrumentType::Spot` on every intent. `StrategyContext.instrument_type` now comes from bot config; orders keep the intent’s type. TSM `effective_position_qty` = filled + signed working qty; subscriber applies fills idempotently (including futures) so a fill cannot zero the book and retrigger the same target. ESM `Position` still overwrites.
