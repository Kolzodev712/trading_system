# Trading System - TODO & Improvement Roadmap

> **Note**: Architectural decisions and feature roadmaps are documented in the **`proposals/`** directory. Significant changes and implementation tracking can also be recorded in **`design-log/`** (see `.cursor/rules/design-log.mdc`).
>
> **Current proposals** (in `proposals/`):
> - **01-indicator-config-generator.md** - Indicator configuration generation from Python
> - **02-strategy-engine.md** - Strategy DSL and IR interpreter
> - **03-dex-preparation.md** - DEX foundational infrastructure
> - **04-solana-integration.md** - Solana DEX implementation
> - **05-future-enhancements.md** - Future enhancements (OCO, TWAP, etc.)
> - **06-private-crate-registry.md** - Private crate distribution
> - **07-hft-subsystem.md** - High-frequency trading daemon
>
> **Completed (no proposal file in repo)**:
> - Futures integration, Bybit integration → implemented in `exchange_engine` (Binance + Bybit)
> - Traceability/observability → `libs/trading_trace`
> - Communication layer / gateway → `src/gateway` (gRPC), `libs/trading_networking` (Zenoh events/commands)
>
> For implementation tasks, refer to the individual proposals and to design logs when using the design-log workflow.

---

## Quick Reference: Proposal Coverage

### ✅ Covered in Proposals

- **Futures Integration** → Proposals 01, 02, 03
- **Advanced Order Types** → Proposals 01, 02, 03 (stop-loss, take-profit, trailing stop)
- **Risk Management** → Proposals 01, 10 (circuit breakers, kill switches)
- **Market Data** → Proposals 01, 02, 03 (funding rate, mark price, etc.)
- **Observability** → ✅ **COMPLETED** - Implemented as `libs/trading_trace` library (see [README](libs/trading_trace/README.md))
- **Web UI Architecture** → Proposal 04 (Gateway layer) ✅ **Implemented**: Gateway crate for gRPC, trading_networking for Zenoh events/commands
- **HFT System** → Proposal 10 (complete HFT daemon design)
- **DEX Integration** → Proposals 03, 04 (Solana DEX)

### 🔮 Future Enhancements (Not Yet in Proposals)

Planned but not yet fully detailed; can be new proposals or extensions (see **05-future-enhancements.md**):

- **OCO, TWAP, VWAP Orders** - Advanced order types
- **Commercial Features** - Multi-tenancy, licensing, billing (separate concern)
- **Advanced Error Handling** - Implementation-phase details
- **Performance Optimizations** - Exchange-specific optimizations

---

**Last Updated**: March 2025  
**Status**: Proposals in `proposals/`; implementation notes in `design-log/` when using design-log workflow.



Traders's advice:

### Trim config and allocate most of the strategy logic instead of hardcoded config. It should be defined from the script. Only bare rudiments should remain in the system config. 

### There should be a clear separation between execution layer (pair data and market info) and strategy computation layer (the strategy desicions and)

Goals:

Selling the system,
Selling the system + consultancy and tech support => A specialized team of a trader, dev and infra guys or something
Trade ourselves => dont