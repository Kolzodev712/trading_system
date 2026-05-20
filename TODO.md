# Trading System - TODO & Improvement Roadmap

> **Note**: Architectural decisions and feature roadmaps are documented in the **`proposals/`** directory. Significant changes and implementation tracking can also be recorded in **`design-log/`** (see `.cursor/rules/design-log.mdc`).
>
> **Current proposals** (in `proposals/`, in roadmap order):
> - **01-solana-integration.md** - Solana DEX (chain, liquidations)
> - **02-strategy-engine.md** - Strategy DSL and IR interpreter
> - **03-future-enhancements.md** - Future enhancements (OCO, TWAP, etc.)
> - **04-private-crate-registry.md** - Private crate distribution
> - **05-hft-subsystem.md** - High-frequency trading daemon
>
> **Completed** (design log / docs; proposal removed when shipped):
> - Indicator config generator → [design log 001](.cursor/design-log/001-indicator-config-generator.md), `python/indicator_config_generator/`
> - DEX venue scaffold → [design log 002](.cursor/design-log/002-dex-preparation.md)
> - Production platform readiness → [design log 003](.cursor/design-log/003-production-platform-readiness.md) (trace, bridge, health, config)
> - Futures integration, Bybit integration → implemented in `exchange_engine` (Binance + Bybit)
> - Traceability/observability → `libs/trading_trace`
> - Communication layer / gateway → `src/gateway` (gRPC), `libs/trading_networking` (Zenoh events/commands)
>
> For implementation tasks, refer to the individual proposals and to design logs when using the design-log workflow.

---

## Quick Reference: Proposal Coverage

### ✅ Covered in Proposals

- **DEX / Solana** → Proposal 01 (after shipped design logs 002–003)
- **Strategy DSL** → Proposal 02
- **Advanced order types (future)** → Proposal 03
- **HFT System** → Proposal 05
- **Observability / platform** → ✅ **COMPLETED** — design log 003, `libs/trading_trace`, `runner::event_bridge`
- **Web UI / gRPC / Zenoh** → ✅ **Implemented**: `src/gateway`, `libs/trading_networking`

### 🔮 Future Enhancements (Not Yet in Proposals)

Planned but not yet fully detailed; can be new proposals or extensions (see **03-future-enhancements.md**):

- **OCO, TWAP, VWAP Orders** - Advanced order types
- **Commercial Features** - Multi-tenancy, licensing, billing (separate concern)
- **Advanced Error Handling** - Implementation-phase details
- **Performance Optimizations** - Exchange-specific optimizations

---

**Last Updated**: May 2026  
**Status**: Proposals in `proposals/`; implementation notes in `design-log/` when using design-log workflow.



Traders's advice:

### Trim config and allocate most of the strategy logic instead of hardcoded config. It should be defined from the script. Only bare rudiments should remain in the system config. 

### There should be a clear separation between execution layer (pair data and market info) and strategy computation layer (the strategy desicions and)

Goals:

Selling the system,
Selling the system + consultancy and tech support => A specialized team of a trader, dev and infra guys or something
Trade ourselves => dont
