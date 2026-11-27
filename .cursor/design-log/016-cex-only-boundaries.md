# CEX-only boundaries (compile DAG + future DEX separation)

## Problem / context

Compile times are dominated by a tangled dependency DAG: `trading_engine` / `runner` / `gateway` pull `exchange_engine` (all CEX adapters) and `dex_engine` (Sui/DeepBook) even when only a thin state API or a CEX Paper/Demo path is needed. DEX venues are **not planned or integrated** as a product; DEX code is dead weight on the CEX build. Long-term we want **separate CEX and DEX systems** that share only reusable libraries — not one binary with dual mode, and **no shared `platform_*` process crate**.

## Decision

Run a sequenced program with **CEX as the only live system** until a future DEX product exists.

### End state (when DEX someday exists)

- Two independent applications (separate runners/gateways/engines).
- Shared **only** pure reusable libs (e.g. types, proto, infra helpers) with **no mutual app edges**.
- No shared `platform_*` crate — duplicate boring main/shutdown/metrics wiring per app.
- CEX must not depend on DEX crates; DEX must not depend on CEX crates.

### Near-term (now — DEX not planned)

1. **Quarantine DEX off the CEX compile DAG**  
   Feature-gate or stop depending on `dex_engine` from `trading_engine`, `runner`, `gateway`, and related paths so default CEX builds do not compile Sui/DeepBook. Leave DEX sources in-tree if useful as archive, but not on the default graph.

2. **Thin CEX exchange API (primary compile win)**  
   Move the small surface `trading_engine` actually needs from `exchange_engine` into a thin shared place (prefer `trading_core` module or a tiny `cex_exchange_api` crate):
   - `ExchangeStateManager` (or a trait + type aliases it needs)
   - `ImmediateTrigger`  
   `exchange_engine` implements/owns adapters and continues to construct/feed state.  
   `trading_engine` depends on the thin API **only**, not the full adapter crate.

3. **`trading_core` kitchen-sink split (after 1–2)**  
   Continue from `trading_proto` (already done):
   - `trading_types` — pure types/traits/channels  
   - `trading_infra` — retry / rate limit / circuit breaker  
   - `trading_store` — mongo/redis behind features  
   Acceptance metric is **compile DAG / incremental rebuild cost**, not module purity aesthetics.

4. **Explicitly out of scope**  
   - Shared `platform_*` crate  
   - Dual CEX+DEX trait symmetry  
   - Building a DEX product in this program  

## Alternatives

- **Cargo features `cex`/`dex` on one runner forever** — rejected as end state (couples products); optional only as a short quarantine tactic before default-off DEX.
- **Extract `platform_*` for shutdown/metrics** — rejected; DEX not planned, duplication is cheaper than a fake shared runtime.
- **Big-bang delete all DEX + full core split in one PR** — rejected; hard to bisect; prefer additive extract → switch → gate/delete edges.
- **Leave `trading_engine → exchange_engine` and only split `trading_core`** — rejected as first step; fat adapter edge is the louder compile cost.

## Consequences

- Default workspace checks/CI should become CEX-focused; DEX targets may need `--features dex` or a separate package set if retained.
- Renames/moves are allowed; **behavior parity** on Paper/Demo CEX path is the gate.
- Future DEX work starts as a **new app** depending on shared libs, not by re-enabling a mode flag in the CEX runner.
- Short-term churn in `Cargo.toml` / imports across `trading_engine`, `runner`, `gateway`.

## Verification

- **Compile DAG:** `cargo tree -p trading_engine` / `-p runner` (default features) must not list `dex_engine` or Sui git deps.
- **Timing:** before/after `cargo check -p trading_engine -j1` and `cargo check -p runner -j1` (cold and incremental after a one-line CEX edit).
- **Thin API:** `trading_engine` `Cargo.toml` has no `exchange_engine` path dep (or only via a feature used by integration tests — prefer none).
- **Behavior:** existing `cargo test -p trading_engine --lib`, `cargo test -p exchange_engine --lib`, Paper/Demo smoke unchanged.
- **Gates:** keep `just test-5r-graph` / research isolation; add a small check that default CEX packages do not depend on `dex_engine`.
- **Core split (later phase):** each extracted crate builds alone; dependents use `workspace = true`; no increase in duplicate proto builds.

## Implementation plan (phased)

### Phase A — Quarantine DEX (small)

1. Inventory `dex_engine` / `src/trading_engine/src/dex` / DeepBook edges from runner, gateway, market_data, trading_engine.
2. Default-off: remove path deps or wrap in `feature = "dex"` defaulted false on CEX packages.
3. Fix compile breaks by cfg-gating DEX modules or moving them behind the feature.
4. Record tree/timing baseline vs after.

### Phase B — Thin CEX API (medium)

1. List all `exchange_engine::` imports from `trading_engine` (known: `ExchangeStateManager`, `ImmediateTrigger` in state/strategies).
2. Introduce thin API module/crate; move or re-export types; `exchange_engine` keeps adapter-side construction.
3. Repoint `trading_engine` imports; drop `exchange_engine` dependency.
4. Confirm `cargo tree -p trading_engine` no longer pulls venue adapter crates (ring/reqwest stacks may remain via `trading_core` until core split).

### Phase C — `trading_core` split (large, later)

1. Feature-gate store/redis/mongo on `trading_core` if not already.
2. Extract `trading_types` → switch imports → delete duplicates.
3. Extract `trading_infra`, then `trading_store`.
4. One crate boundary per PR; workspace check + gate-5r after each.

### Phase D — Future DEX (explicit non-goal now)

New repository or workspace app; depends on shared libs only; no edge into CEX runner/gateway.

## Implementation results

_(append after each phase: what shipped, tree/timing deltas, deviations)_

### Phase A — Quarantine DEX (shipped, uncommitted)

**What shipped**
- `dex_engine` is now `optional = true` in `trading_engine`, `market_data_engine`, `gateway`, `runner`; each exposes opt-in feature `dex` (default off). Chain: `runner/dex` → `trading_engine/dex`, `gateway/dex`, `market_data_engine/dex`; `gateway/dex` → `trading_engine/dex`. `dex_engine` and `scripts/deepbook` remain workspace members.
- Gated code: `trading_engine::dex`, `market_data_engine::exchanges::deepbook` + `ExchangeConfig::DeepBook` + driver arm, `runner::dex`, `SharedServices.dex_indicator_engine`, `BotConfig.dex_engine` (JSON key ignored without the feature), `VenueInterface::{Dex,Both}` / `DexVenueHandle`, DEX helpers in `platform/init.rs`, DEX fields/builders in gateway `TradingCliServiceImpl` / `MonitoringService`.
- Without the feature: `VenueMode::Dex|Both` and runner modes `dex` / `dex-strategies` fail validation with `"DEX support not enabled; build with --features dex"`; `platform/metrics.rs` reports `dex_execute_*` as 0; gateway DEX RPCs (`toggle_dex_quote`, `cancel_dex_pool_orders`, `toggle_dex_strategy`) return `failed_precondition` with the same message; DEX monitoring snapshot is `None`.
- `VenueInterface` gained `cex_handle()` / `has_dex()`; `dex_update_rx()` stays available (returns `None` without the feature) since `DexVenueUpdateReceiver` lives in `trading_core`.

**Tree check (default features)**
- `cargo tree -p trading_engine|runner|gateway|market_data_engine|btc_price_ma_app -e normal` → no `dex_engine`, no Sui/DeepBook crates. `cargo tree -p runner --features dex` → `dex_engine` present (4 hits).

**Deviations / notes**
- Root `trading_system` package (integration tests) has `dex_engine` as a **dev-dependency** (cannot be optional), so `cargo test -p trading_system` still compiles `dex_engine`; this does not touch the runtime graph above. Added root feature `dex = [trading_engine/dex, runner/dex, market_data_engine/dex]` and gated `tests/integration/internal/dex` + the `dex_*` fields in `strategy.rs` literals.
- Three runner `config_smoke_tests` that load DEX example bot configs are `#[cfg(feature = "dex")]`; added a no-feature test asserting the clear error.
- `gateway/dex` does not enable `market_data_engine/dex` (gateway never touches DeepBook MD); `runner/dex` does, per spec.
- `cargo fmt` only touched edited files. No timing baseline captured in Phase A (not requested).

**Verification**
- `cargo check -p trading_engine -p runner -p gateway -p market_data_engine -p btc_price_ma_app --all-targets` and `cargo check --workspace --all-targets` OK.
- `cargo check -p trading_engine --features dex`, `-p runner --features dex --all-targets`, `-p gateway --features dex --all-targets`, `-p market_data_engine --features dex --all-targets`, `-p dex_engine`, `-p trading_system --tests --features dex` OK.
- `cargo test -p trading_engine --lib` (196), `-p market_data_engine --lib` (42), `-p runner --lib` (39 default / 41 with `dex`), `-p gateway --lib` (25) pass.
- `cargo clippy -D warnings` clean on the CEX packages (default features) and on `runner`/`market_data_engine` with `--features dex`.
