# Trading System - TODO & Improvement Roadmap

> **Note**: This file has been superseded by the comprehensive proposal documents. All architectural decisions, implementation plans, and feature roadmaps are now documented in the `proposals/` directory.
>
> **Current Proposals**:
> - **01-futures-integration.md** - Futures types, traits, and infrastructure
> - **03-bybit-integration.md** - Bybit unified driver (spot + futures)


> - ~~**03-traceability-observability.md**~~ - ✅ **COMPLETED** - Implemented as `libs/trading_trace` library
> - ~~**04-communication-layer-hexagonal-architecture.md**~~ - ✅ **COMPLETED** - Gateway crate (gRPC), `libs/trading_networking` (Zenoh events/commands); proposal doc removed after implementation
> - **05.01-dex-preparation.md** - DEX foundational infrastructure
> - **05.02-solana-integration.md** - Solana DEX implementation
> - **07-indicator-config-generator.md** - Indicator configuration generation from Python
> - **08-strategy-engine.md** - Strategy DSL and IR interpreter
> - **09-private-crate-registry.md** - Private crate distribution
> - **10-hft-subsystem.md** - High-frequency trading daemon
>
> For implementation tasks and tracking, refer to the individual proposals and their implementation phases.

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
- **DEX Integration** → Proposals 05.01, 05.02 (Solana DEX)

### 🔮 Future Enhancements (Not Yet in Proposals)

These items are planned but not yet detailed in proposals. They can be added as new proposals or enhancements to existing ones:

- **OCO, TWAP, VWAP Orders** - Advanced order types (mentioned in Proposal 01 future enhancements)
- **Commercial Features** - Multi-tenancy, licensing, billing (separate concern, future proposal)
- **Advanced Error Handling** - Implementation details (handled during implementation phase)
- **Performance Optimizations** - Exchange-specific optimizations (implementation details)

---

**Last Updated**: December 2024  
**Status**: Superseded by proposal documents