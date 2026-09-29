# Architecture Documentation

**Current reference (2026-09-29):** [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) and [docs/Trading-System-Analysis.txt](../docs/Trading-System-Analysis.txt). Increment record: [docs/PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md](../docs/PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md).

The decoupling, three-repository split, Paper qualification (Increment 9A), and bounded Bybit Demo qualification (Increment 10, synthetic signal) are **complete at that stated scope**. The system is **not** documented as production complete or mainnet ready.

**Explicitly open:** journal and recover FlattenAndPause’s reduce-only submission; propagate a CancelWorking failure from FlattenAndPause; validate Candidate #2 as a strategy; separate mainnet readiness and approval.

The numbered files in this directory describe **in-process engines**. They can lag crate layout (extracted `strategy_core` / `research_engine`, `btc_price_ma_app`, gateway, operator RPCs). [architecture.dsl](architecture.dsl) is a pre-decoupling engine view. [10-strategy-engine.md](10-strategy-engine.md) is a proposed Python DSL/IR path, not Candidate #2.

## Overview

Live CEX services still run in-process and communicate on broadcast channels. That monolith is one **container** inside `trading_system`. It is not the whole system: k-paper and trading-system-ui are separate git repositories.

## Documents

### Current system (start here)

0. **[Current architecture](../docs/ARCHITECTURE.md)** — decoupling, three repos, Paper, Demo, open items

### Core services (in-process engines)

1. **[Exchange Engine](01-exchange-engine.md)** — exchange account data, WS/REST, reconciliation, Binance/Bybit/Paper
2. **[Market Data Engine](02-market-data-engine.md)** — public orderbooks/trades
3. **[Trading Engine](03-trading-engine.md)** — execution state; generic `Strategy` trait; **does not** own Candidate #2 (see `strategy_core` + `btc_price_ma_app`)
4. **[Indicator Engine](04-indicator-engine.md)** — SMA/EMA and related caches
5. **[Risk Engine](05-risk-engine.md)** — independent checks / circuit breaker
6. **[Asset Engine](06-asset-engine.md)** — instrument metadata

### Supporting services

7. **[Live Config Service](07-live-config-service.md)** — runtime config updates
8. **[Runner](08-runner.md)** — orchestrator; gRPC via **gateway**; Zenoh **opt-in** (`ZENOH_ENABLED=1`); Candidate #2 via `btc_price_ma_app`
9. **[TUI CLI](09-tui-cli.md)** — legacy terminal client; **not** the planned operator layer (`trading-system-ui` is)

### Other

10. **[Strategy Engine (proposed DSL)](10-strategy-engine.md)** — Python IR proposal; not the Increment 0–10 Candidate #2 path
11. **[Docker Guide](DOCKER.md)**
12. **[Structurizr](STRUCTURIZR.md)** — historical engine diagrams; not current qualification status

## Architecture principles

### Crate decoupling (implemented)

- **`strategy_core`**: pure model. No `TradingClient`, exchange, or research accounting.
- **`research_engine` / `research_cli`**: historical evaluation only.
- **`btc_price_ma_app`**: composition root; injects Candidate #2 into the generic runner.
- **`runner` / `trading_engine`**: no dependency on `strategy_core`, research crates, or the app crate.
- **`gateway`**: gRPC proto and `TradingCliService` (`PauseEntries`, `CancelWorking`, `FlattenAndPause`, `Resume`).

### Three repositories

| Repo | Authority |
| --- | --- |
| `trading_system` | Models, research, live state, proto |
| `k-paper` | Paper exchange behavior |
| `trading-system-ui` | Browser UX only |

### Communication

- **Channels**: in-process broadcast (source of truth inside the runner)
- **gRPC**: UI/TUI via gateway; mutating non-Paper RPCs require `TRADING_OPERATOR_TOKEN` at Envoy/runner
- **Zenoh**: opt-in (`ZENOH_ENABLED=1`); not the Paper/Demo qualification path
- **REST/WS**: exchange and k-paper adapters

### Key engine principles (still true)

1. Exchange Engine is the single point of contact with the exchange for account data
2. Trading Engine relies on ExchangeStateManager — it does not call the exchange directly
3. Risk Engine is an independent checker
4. Flatten RPC success is not flat; independent venue+runner verification is required

## Related

- [Venue modes](../docs/VENUE_MODES.md)
- [Command plane](../docs/COMMAND_PLANE.md)
- [Platform data plane](../docs/PLATFORM_DATA_PLANE.md)
