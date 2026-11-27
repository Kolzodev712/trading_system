# DEX venue scaffold (shipped)

> **Next work:** Platform readiness → [design log 003](003-production-platform-readiness.md) (shipped). **Solana:** [Proposal 01](../../proposals/01-solana-integration.md).

## Problem / context

Need a hard **CEX / DEX split**: CEX behavior (Paper/Binance/Bybit, levels, `TradingClient`) must not change. DEX (Solana liquidations first) needs its own crate and types, not extensions bolted onto `exchange_engine` or scattered CEX traits in `trading_core`.

## Decision

1. **`trading_core::venue::cex`** — all CEX venue code (traits, `ExchangeUpdate`, order types) moved from crate-root `traits/`, `channels/exchange`, `types/exchange`; root modules become `pub use` shims only.
2. **`trading_core::venue::dex`** — `VenueProvider`, `TradeCommand`, `DexVenueUpdate`; nothing DEX in root `traits/` or `types/exchange`.
3. **`dex_engine`** — parallel to `exchange_engine`; shared state/run/reconcile; **`chains/solana/`** (etc.) for chain-specific code.
4. **`exchange_engine`** — CEX only; `VenueInterface::Cex` wraps current stack unchanged.
5. **`trading_engine::dex`** — DEX strategies (liquidations); **`levels/`** stays CEX-only.
6. **No** DEX fields on `ExchangeState`; separate `DexStateManager`.

## Alternatives

- Extend `ExchangeState` + single `ExecutionInterface` (old Proposal 01 v1) — rejected; couples venues and risks CEX regressions.
- `dex_engine` without `venue/` in `trading_core` — rejected; leaves CEX traits at crate root.

## Consequences

- Runner/gateway eventually wire two monitors; v1 config can remain CEX-only until 02 lands.
- `market_data_engine` gains DEX feed path (subdir), separate from CEX WS clients.
- One-time migration churn for `trading_core` imports; mitigated with `pub use` shims.

## Verification

- Phase 1: CEX tests + k-paper levels unchanged after `venue/` re-exports.
- `cargo test -p dex_engine` with mock `VenueProvider`.
- Phase 3 (02): devnet liquidation path through `dex_engine/chains/solana` only.

[cursor-hub workflow] step=design-review

## Implementation results

- `trading_core::venue::{cex,dex}`: CEX traits/types/channels moved under `venue/cex`; `ExchangeUpdate` renamed to `CexVenueUpdate`; root `traits/` + `channels/exchange` + `types/exchange` are shims.
- `DexError`, `TradeCommand`, `DexVenueUpdate`, `VenueProvider` in `venue/dex`.
- New crate `dex_engine` with `DexStateManager`, `run()`, `chains/mock`.
- Runner: `venue.mode` in `bot.json`, `VenueInterface` in `runner::venue`, `SharedServices.venue`, `dex_engine` optional config block.
- Example: `config/bot.dex.example.json`.
- CEX paths unchanged at runtime when `venue.mode = cex` (default).
- Completed (no Solana): `MarketDataUpdate::DexAccountHealth`, `DexEngineRuntime`, reconciler loop, `trading_engine::dex` stub, `venue.mode: both`, runner `dex` mode, gRPC `MonitoringData.dex`, removed duplicate `traits/*.rs`.

[cursor-hub workflow] step=implementation
