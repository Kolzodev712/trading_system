# Architecture Documentation

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

This directory contains detailed architectural documentation for each service and component of the trading system. Each document provides comprehensive information about the service's architecture, components, data flow, and usage.

## Documents

### Core Services

1. **[Exchange Engine](01-exchange-engine.md)**
   - Single point of contact with exchange for account data
   - WebSocket and REST API integration
   - State management and reconciliation
   - Position tracking for futures trading
   - Multi-exchange support (Binance, Bybit)
   - Rate limiting (REQUEST_WEIGHT, ORDERS) with automatic enforcement
   - Order amendment API (amend instead of cancel+place)

2. **[Market Data Engine](02-market-data-engine.md)**
   - Public market data management (orderbooks, trades)
   - Real-time data streaming
   - Channel-based publishing

3. **[Trading Engine](03-trading-engine.md)**
   - Core trading logic and execution
   - Level-based and strategy-based trading
   - State management and risk integration
   - Automatic order amendments (amend when price/quantity changes)

4. **[Indicator Engine](04-indicator-engine.md)**
   - Technical indicator calculation (SMA, EMA, composite)
   - Real-time indicator values
   - Caching and performance optimization

5. **[Risk Engine](05-risk-engine.md)**
   - Comprehensive risk management
   - Circuit breaker pattern
   - Independent verification capability

6. **[Asset Engine](06-asset-engine.md)**
   - Static asset metadata caching
   - Trading rules and exchange limits
   - TTL-based refresh

### Supporting Services

7. **[Live Config Service](07-live-config-service.md)**
   - Runtime configuration updates
   - Validation and audit logging
   - Channel-based API

8. **[Runner](08-runner.md)**
   - Application orchestrator
   - Entry point: `runner::run()` in lib; thin `main.rs` binary
   - gRPC via **gateway** crate (in-process or optional standalone proxy)
   - Zenoh event gateway and command queryable (always on)
   - Service initialization and coordination

9. **[TUI CLI](09-tui-cli.md)**
   - Terminal user interface
   - Real-time monitoring
   - Separate views for Spot and Futures trading
   - Position monitoring for futures (leverage, margin, P&L)
   - Level management and trading control

### Deployment & Operations

10. **[Docker Guide](DOCKER.md)**
    - Docker containerization
    - Docker Compose setup
    - Running the system and CLI in containers
    - Troubleshooting and common commands

## Architecture Principles

### State Management

- **State Managers**: Exchange, Market Data, Trading - manage full state and publish
- **Caching Services**: Indicator, Risk, Asset - fast lookup, no state management

### Communication

- **Channels**: Ultra-low latency (< 1μs) in-process communication
- **gRPC**: External communication (TUI CLI); implementation in **gateway** crate; runner spawns server or uses standalone gateway proxy
- **Zenoh**: External events (hot/cold) and commands (queryable); always on when runner is active
- **REST API**: Exchange communication (account data, market data)

### Gateways and external interfaces

- **Gateway crate** (`src/gateway`): Owns all gRPC (proto, server, service handlers). Runner uses it to expose the TUI CLI API; optional standalone binary for gRPC proxy.
- **Trading networking** (`libs/trading_networking`): Event gateway and Zenoh transport (events + commands). Runner wires it and starts Zenoh on every run.

### Key Principles

1. **Exchange Engine is the single point of contact** with exchange for account data
2. **Trading Engine relies on ExchangeStateManager** - does NOT call exchange directly
3. **Risk Engine is an independent checker** - can verify state correctness
4. **State Managers** manage full state and publish
5. **Caching Services** provide fast lookup, no state management

## Data Flow

### Exchange Data Flow

```
Exchange WebSocket → ExchangeStateManager
    ├──→ Trading Engine (via channels)
    ├──→ Risk Engine (via channels)
    └──→ TUI (via gRPC aggregation)
```

### Market Data Flow

```
Exchange WebSocket → MarketDataStateManager
    ├──→ Indicator Engine (via channels)
    ├──→ Trading Engine (via channels)
    └──→ TUI (via gRPC aggregation)
```

### Indicator Data Flow

```
MarketDataStateManager → Indicator Engine
    ├── Calculate indicators
    └──→ Trading Engine (via channels)
        └──→ TUI (via gRPC aggregation)
```

## Performance Characteristics

- **Channel Latency**: < 1μs (in-process)
- **gRPC Latency**: < 10ms (external)
- **Risk Checks**: < 1ms (cached)
- **Indicator Lookup**: < 0.1ms (cached)

## Testing

Each service has:
- Unit tests for core functionality
- Integration tests for service interactions
- Test helpers for common setup

## Configuration

All services are configured via:
- `config/bot.json`: Main configuration
- `config/strategies.json`: Strategy configuration
- `config/levels.json`: Levels configuration

## Version History

- **0.1** (December 2024): Initial architecture documentation

## Related Documentation

- [README](../README.md): User guide and usage instructions
- [Docker Guide](DOCKER.md): Docker deployment guide

