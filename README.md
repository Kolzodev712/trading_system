# Trading System

**Version**: 0.1  
**Last Updated**: December 2024  
**License**: MIT License  
**Copyright**: © 2024 Trading System Contributors

A modular, exchange-agnostic trading system built in Rust with support for real-time market data, order execution, level-based trading, and strategy-based trading.

## Table of Contents

- [Overview](#overview)
- [System Architecture](#system-architecture)
- [Services Overview](#services-overview)
- [Prerequisites & Dependencies](#prerequisites--dependencies)
- [Installation](#installation)
- [Configuration](#configuration)
- [Running the System](#running-the-system)
- [Testing](#testing)
- [Maintenance](#maintenance)
- [Troubleshooting](#troubleshooting)
- [Architecture Documentation](#architecture-documentation)

## Overview

The Trading System is a comprehensive, production-ready trading platform that supports:

- **Level-Based Trading**: Predefined trading levels with indicator-based price/quantity resolution
- **Strategy-Based Trading**: Algorithmic trading strategies (e.g., BuyLowSellHigh)
- **Real-Time Market Data**: WebSocket-based orderbook and trade streaming
- **Risk Management**: Comprehensive risk checks, circuit breakers, and daily loss limits
- **TUI CLI**: Terminal-based interface for monitoring and level management
- **Exchange Integration**: Supports Binance (spot) and Bybit (spot and futures), extensible to other exchanges

### Key Features

- ✅ **Exchange-Agnostic Design**: Easy to add new exchanges
- ✅ **Ultra-Low Latency**: Channel-based communication (< 1μs)
- ✅ **State Management**: Full state reconciliation and persistence
- ✅ **Real-Time Monitoring**: TUI CLI with live data streaming
- ✅ **Risk Management**: Circuit breakers, position limits, daily loss limits
- ✅ **Level Management**: Full CRUD operations via TUI or gRPC
- ✅ **Indicator Engine**: Real-time technical indicators (SMA, EMA, composite)
- ✅ **Rate Limiting**: Automatic rate limit enforcement with 429 error handling
- ✅ **Order Amendments**: Automatic order amendments when price/quantity changes (no cancel+place)
- ✅ **Distributed Tracing**: Hybrid tracing system with OpenTelemetry integration
- ✅ **Comprehensive Testing**: Unit tests and integration tests

## System Architecture

The system follows a **monolithic architecture** with clear service boundaries. All services run in-process and communicate via ultra-low latency broadcast channels.

### High-Level Architecture

<details>
<summary><b>📊 Interactive Architecture Diagram</b> (Click to expand)</summary>

> **💡 Tip**: For interactive diagrams with zoom, pan, and filtering, use the [Structurizr architecture diagram](architecture/STRUCTURIZR.md). You can also export it to various formats (Mermaid, PlantUML, PNG, SVG).

#### System Context Diagram

```mermaid
graph TB
    Trader[👤 Trader] -->|Uses| TUI[TUI CLI]
    TUI -->|gRPC| Runner[Runner<br/>gRPC Server]
    
    Runner -->|Orchestrates| ExchangeEngine[Exchange Engine]
    Runner -->|Orchestrates| MarketDataEngine[Market Data Engine]
    Runner -->|Orchestrates| TradingEngine[Trading Engine]
    Runner -->|Orchestrates| IndicatorEngine[Indicator Engine]
    Runner -->|Orchestrates| AssetEngine[Asset Engine]
    Runner -->|Orchestrates| LiveConfig[Live Config Service]
    
    ExchangeEngine -->|WebSocket| Binance[Binance Exchange]
    ExchangeEngine -->|REST API| Binance
    MarketDataEngine -->|WebSocket| Binance
    AssetEngine -->|REST API| Binance
    TradingEngine -->|Places Orders| Binance
    
    Runner -->|Persists Config| MongoDB[(MongoDB)]
    TradingEngine -->|Persists Levels| MongoDB
    IndicatorEngine -->|Persists Indicators| MongoDB
    LiveConfig -->|Persists Updates| MongoDB
    
    ExchangeEngine -.->|Broadcast Channel<br/>< 1μs| TradingEngine
    ExchangeEngine -.->|Broadcast Channel<br/>< 1μs| TradingEngine
    MarketDataEngine -.->|Broadcast Channel<br/>< 1μs| IndicatorEngine
    MarketDataEngine -.->|Broadcast Channel<br/>< 1μs| TradingEngine
    IndicatorEngine -.->|Broadcast Channel<br/>< 1μs| TradingEngine
    
    TradingEngine -->|Gets Rules| AssetEngine
    TradingEngine -->|Checks Risk| TradingEngine
    
    classDef stateManager fill:#438dd5,stroke:#08427b,color:#fff
    classDef cache fill:#85bbf0,stroke:#438dd5,color:#000
    classDef orchestrator fill:#08427b,stroke:#1168bd,color:#fff
    classDef external fill:#999,stroke:#666,color:#fff
    
    class ExchangeEngine,MarketDataEngine,TradingEngine stateManager
    class IndicatorEngine,AssetEngine cache
    class Runner orchestrator
    class TUI,Binance,MongoDB external
```

#### Container View (Services)

```mermaid
graph LR
    subgraph TradingSystem["🏦 Trading System (Monolith)"]
        direction TB
        ExchangeEngine[Exchange Engine<br/>State Manager]
        MarketDataEngine[Market Data Engine<br/>State Manager]
        TradingEngine[Trading Engine<br/>State Manager]
        IndicatorEngine[Indicator Engine<br/>Cache]
        AssetEngine[Asset Engine<br/>Cache]
        LiveConfig[Live Config Service]
        Runner[Runner<br/>Orchestrator]
    end
    
    subgraph External["🌐 External Systems"]
        TUI[TUI CLI]
        Binance[Binance Exchange]
        MongoDB[(MongoDB)]
    end
    
    Runner -->|gRPC| TUI
    ExchangeEngine -->|WSS/HTTPS| Binance
    MarketDataEngine -->|WSS| Binance
    AssetEngine -->|HTTPS| Binance
    TradingEngine -->|HTTPS| Binance
    Runner -->|MongoDB Protocol| MongoDB
    
    ExchangeEngine -.->|Channel| TradingEngine
    MarketDataEngine -.->|Channel| IndicatorEngine
    MarketDataEngine -.->|Channel| TradingEngine
    IndicatorEngine -.->|Channel| TradingEngine
    
    classDef stateManager fill:#438dd5,stroke:#08427b,color:#fff
    classDef cache fill:#85bbf0,stroke:#438dd5,color:#000
    classDef orchestrator fill:#08427b,stroke:#1168bd,color:#fff
    classDef external fill:#999,stroke:#666,color:#fff
    
    class ExchangeEngine,MarketDataEngine,TradingEngine stateManager
    class IndicatorEngine,AssetEngine cache
    class Runner orchestrator
    class TUI,Binance,MongoDB external
```

#### Data Flow Diagram

```mermaid
sequenceDiagram
    participant Binance as Binance Exchange
    participant ExchangeEngine as Exchange Engine
    participant MarketDataEngine as Market Data Engine
    participant IndicatorEngine as Indicator Engine
    participant TradingEngine as Trading Engine
    participant TUI as TUI CLI
    participant MongoDB as MongoDB
    
    Binance->>ExchangeEngine: WebSocket (Orders, Balances)
    Binance->>MarketDataEngine: WebSocket (Orderbooks, Trades)
    
    ExchangeEngine->>TradingEngine: Broadcast Channel (< 1μs)
    MarketDataEngine->>IndicatorEngine: Broadcast Channel (< 1μs)
    MarketDataEngine->>TradingEngine: Broadcast Channel (< 1μs)
    IndicatorEngine->>TradingEngine: Broadcast Channel (< 1μs)
    
    TradingEngine->>Binance: REST API (Place Order)
    
    TradingEngine->>MongoDB: Persist Levels
    IndicatorEngine->>MongoDB: Persist Indicators
    
    TradingEngine->>Runner: gRPC (Monitoring Data)
    Runner->>TUI: gRPC Stream
```

</details>

#### ASCII Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│              Trading System (Monolith)                   │
│                                                          │
│  ┌──────────────────┐      ┌──────────────────┐        │
│  │ Exchange Engine  │      │ Market Data     │        │
│  │ (State Manager)  │      │ Engine          │        │
│  │                 │      │ (State Manager) │        │
│  └────────┬────────┘      └────────┬─────────┘        │
│           │                         │                  │
│           │                         │                  │
│  ┌────────▼────────────────────────▼─────────┐        │
│  │         Trading Engine                    │        │
│  │  (State Manager + Execution)             │        │
│  └────────┬──────────────────────────────────┘        │
│           │                                             │
│  ┌────────▼────────┐      ┌──────────────────┐        │
│  │ Indicator       │      │ Risk Engine      │        │
│  │ Engine (Cache)  │      │ (Cache)          │        │
│  └─────────────────┘      └──────────────────┘        │
│                                                          │
│  ┌──────────────────┐      ┌──────────────────┐        │
│  │ Asset Engine     │      │ Live Config      │        │
│  │ (Cache)          │      │ Service          │        │
│  └──────────────────┘      └──────────────────┘        │
│                                                          │
│  ┌──────────────────────────────────────────┐          │
│  │         Runner (Orchestrator)            │          │
│  │  + gRPC Server (for TUI CLI)            │          │
│  └──────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────┘
           │
           │ gRPC
           │
┌──────────▼──────────┐
│    TUI CLI           │
│  (External Process)  │
└──────────────────────┘
```

### Communication Patterns

- **In-Process**: Broadcast channels (ultra-low latency, < 1μs)
- **External**: gRPC (for TUI CLI communication)
- **Exchange**: WebSocket + REST API (for exchange communication)

### View Interactive Diagrams

For interactive architecture diagrams with zoom, pan, and filtering capabilities:

1. **Structurizr Diagrams**: See [STRUCTURIZR.md](architecture/STRUCTURIZR.md) for instructions on viewing the full Structurizr architecture diagrams
2. **Export Options**: Export to Mermaid, PlantUML, PNG, SVG formats
3. **Component Details**: View detailed component-level diagrams for each service

For detailed architecture information, see the [Architecture Documentation](#architecture-documentation) section.

## Services Overview

### Core Services

#### 1. Exchange Engine
**Purpose**: Single point of contact with exchange for account data (balances, orders, trades).

**Key Features**:
- WebSocket and REST API integration
- State management with reconciliation
- Broadcast channel publishing
- Exchange-agnostic design
- **Rate limiting**: Automatic rate limit enforcement (REQUEST_WEIGHT, ORDERS)
- **429 handling**: Automatic retry with `retryAfter` support
- **Order amendments**: Native order modification API (amend instead of cancel+place)

**See**: [Exchange Engine Architecture](architecture/01-exchange-engine.md)

#### 2. Market Data Engine
**Purpose**: Manages public market data (orderbooks, public trades).

**Key Features**:
- Real-time orderbook streaming
- Public trade streaming
- Channel-based publishing
- Symbol registry integration

**See**: [Market Data Engine Architecture](architecture/02-market-data-engine.md)

#### 3. Trading Engine
**Purpose**: Core trading logic and execution (levels and strategies).

**Key Features**:
- Level-based trading with indicator resolution
- Strategy-based trading
- State management and reconciliation
- Risk integration
- **Automatic order amendments**: Amends existing orders when price/quantity changes instead of canceling and placing new

**See**: [Trading Engine Architecture](architecture/03-trading-engine.md)

#### 4. Indicator Engine
**Purpose**: Calculates and provides real-time technical indicator values.

**Key Features**:
- SMA, EMA, composite indicators
- Real-time calculation
- In-memory caching (< 0.1ms lookup)
- Extensible framework

**See**: [Indicator Engine Architecture](architecture/04-indicator-engine.md)

#### 5. Risk Engine
**Purpose**: Comprehensive risk management for all trading operations.

**Key Features**:
- Circuit breaker pattern
- Daily loss limits (global and per-pair)
- Position and order limits
- Independent verification capability

**See**: [Risk Engine Architecture](architecture/05-risk-engine.md)

#### 6. Asset Engine
**Purpose**: Caches static asset metadata (symbols, trading rules, exchange limits).

**Key Features**:
- TTL-based caching (5 minutes default)
- Trading rules for order validation
- Exchange limits
- Priority system (exchange values override config)

**See**: [Asset Engine Architecture](architecture/06-asset-engine.md)

### Supporting Services

#### 7. Live Config Service
**Purpose**: Runtime configuration updates without restart.

**Key Features**:
- Channel-based API (no HTTP dependencies)
- Validation and audit logging
- MongoDB persistence
- Level and risk limit updates

**See**: [Live Config Service Architecture](architecture/07-live-config-service.md)

#### 8. Runner
**Purpose**: Application orchestrator and gRPC server.

**Key Features**:
- Service initialization and coordination
- Dual mode support (strategies/levels)
- gRPC server for external communication
- Graceful shutdown management

**See**: [Runner Architecture](architecture/08-runner.md)

#### 9. TUI CLI
**Purpose**: Terminal user interface for monitoring and level management.

**Key Features**:
- Real-time monitoring via gRPC streaming
- Level management (CRUD operations)
- Trading control (enable/disable)
- Level editing and creation

**See**: [TUI CLI Architecture](architecture/09-tui-cli.md)

### Core Libraries

#### Trading Trace Library
**Purpose**: High-performance distributed tracing and observability.

**Key Features**:
- **Hybrid Tracing**: Hot path (<1μs) for high-frequency operations, Cold path (<10μs) for commands/events
- **OpenTelemetry Integration**: Full OpenTelemetry SDK with Jaeger and Prometheus exporters
- **Correlation IDs**: Link related operations across services
- **Prometheus Metrics**: Pre-configured metrics for trading operations
- **Zero-Copy Design**: Minimal allocations for maximum performance

**See**: [Trading Trace Library README](libs/trading_trace/README.md) for detailed documentation, API reference, usage examples, and integration guide.

## Prerequisites & Dependencies

### Required Services

#### 1. MongoDB
**Purpose**: Configuration persistence (levels, risk limits, indicators).

**Version**: MongoDB 3.0+ (or compatible)

**Installation**:
```bash
# Using Docker
docker run -d -p 27017:27017 --name mongodb mongo:latest

# Using systemd (Linux)
sudo systemctl start mongod

# Using Homebrew (macOS)
brew services start mongodb-community
```

**Configuration**: Connection string in `config/bot.json`
```json
{
  "mongo": {
    "enabled": true,
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system"
  }
}
```

**Collections**:
- `bot_configs` - Bot configurations with levels and risk limits
- `indicators` - Indicator definitions
- `strategies` - Strategy configurations

#### 2. Exchange API Access
**Exchanges**: Binance (spot) and Bybit (spot and futures)

**Required**:
- API Key
- API Secret
- Testnet/Production environment selection

**Permissions Needed**:
- Read account information
- Place orders
- Cancel orders
- View order history
- View trade history

**Configuration**: API keys in `config/bot.json` (see [Configuration](#configuration))

**Note**: Sample configurations are available in `sample_config/binance/` and `sample_config/bybit/` directories.

### Optional Services

#### Redis
**Status**: Optional (only if Web UI feature is enabled in future)

**Note**: Redis is **not required** for TUI CLI. All internal communication uses broadcast channels.

### System Requirements

#### Hardware
- **CPU**: 2+ cores recommended
- **Memory**: 512MB+ RAM (1GB+ recommended)
- **Network**: Stable internet connection with low latency to exchange
- **Disk**: Minimal (config files, logs)

#### Software
- **Rust**: 1.70+ (stable)
- **Cargo**: Included with Rust
- **Operating System**: Linux, macOS, or Windows
- **Terminal**: ANSI-compatible terminal (for TUI CLI)

### Network Requirements

#### Outbound Connections
- **Binance REST API**: `https://api.binance.com` (production) or `https://testnet.binance.vision` (testnet)
- **Binance WebSocket**: `wss://stream.binance.com` (production) or `wss://testnet.binance.vision` (testnet)
- **Bybit REST API**: `https://api.bybit.com` (production) or `https://api-testnet.bybit.com` (testnet)
- **Bybit WebSocket**: `wss://stream.bybit.com` (production) or `wss://stream-testnet.bybit.com` (testnet)
- **MongoDB**: Connection to MongoDB instance (if enabled)

#### Inbound Connections
- **gRPC Server**: Port 50051 (default, configurable)
- **Health Server**: Port 8080 (default, configurable)

#### Firewall Rules
- Allow outbound HTTPS (443) to exchange APIs
- Allow outbound WebSocket (443) to exchange WebSocket endpoints
- Allow inbound connections to gRPC/HTTP ports (if accessing remotely)
- Allow connections to MongoDB (if remote)

## Installation

### 1. Clone the Repository

```bash
git clone <repository-url>
cd trading_system
```

### 2. Install Rust

If Rust is not installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 3. Build the Project

```bash
# Build in release mode (optimized)
cargo build --release

# Or build in debug mode (faster compilation, slower runtime)
cargo build
```

### 4. Docker Installation (Alternative)

The system can be run using Docker containers, which simplifies deployment and dependency management.

**📖 For detailed Docker instructions, see [DOCKER.md](architecture/DOCKER.md)**

#### Prerequisites
- Docker 20.10+ and Docker Compose 2.0+

#### Quick Start with Docker Compose

```bash
# Start all services (MongoDB + Trading System)
docker-compose up -d

# View logs
docker-compose logs -f trading-system

# Stop all services
docker-compose down

# Stop and remove volumes (clean slate)
docker-compose down -v
```

#### Building Docker Images

```bash
# Build runner image
docker build -t trading-system:latest -f Dockerfile .

# Build CLI image
docker build -t trading-system-cli:latest -f Dockerfile.cli .

# Or use the rebuild script (recommended)
./scripts/rebuild.sh              # Quick rebuild (uses cache)
./scripts/rebuild.sh --no-cache   # Full rebuild (no cache)
```

#### Running with Docker

**Trading System Runner:**
```bash
# Run in levels mode (default)
docker run -d \
  --name trading-system \
  -p 50051:50051 \
  -p 8080:8080 \
  -v $(pwd)/config:/app/config:ro \
  --network trading-network \
  trading-system:latest

# Run in strategies mode
docker run -d \
  --name trading-system \
  -p 50051:50051 \
  -p 8080:8080 \
  -v $(pwd)/config:/app/config:ro \
  trading-system:latest \
  --mode strategies --strategies-config /app/config/strategies.json
```

**TUI CLI (Interactive):**
```bash
# Run TUI CLI connected to trading system
docker run -it --rm \
  --network trading-network \
  trading-system-cli:latest \
  http://trading-system:50051

# Or connect to localhost (if runner is on host)
docker run -it --rm \
  --network host \
  trading-system-cli:latest \
  http://localhost:50051
```

#### Docker Architecture

**Why Separate Containers?**

1. **Runner Container**: Long-running service with gRPC server
   - Runs continuously
   - Handles trading logic
   - Exposes gRPC (50051) and health (8080) ports

2. **TUI CLI Container**: Interactive terminal interface
   - Run on-demand when needed
   - Connects to runner via gRPC
   - Requires interactive terminal (`-it` flag)

3. **MongoDB Container**: Configuration persistence
   - Optional but recommended
   - Managed via docker-compose

**Benefits:**
- ✅ Isolated dependencies
- ✅ Consistent environment
- ✅ Easy deployment
- ✅ Resource management
- ✅ Network isolation

**Note**: The TUI CLI is typically run manually when needed, not as a long-running service. Use `docker-compose` for the runner and MongoDB, then run the CLI container interactively when you need to monitor or manage levels.

### 5. Check Dependencies

Use the provided script to check and start dependencies:

```bash
# Check dependencies and ports
./scripts/check_dependencies.sh

# This script will:
# - Check MongoDB connection
# - Check Redis (if enabled)
# - Verify ports are free (gRPC: 50051, Health: 8080)
# - Auto-start missing dependencies in new terminal windows
```

### 5. Configure the System

See [Configuration](#configuration) section for detailed configuration instructions.

## Configuration

### Configuration Files

The system uses three main configuration files:

1. **`config/bot.json`**: Main bot configuration
2. **`config/strategies.json`**: Strategy-specific configuration (for strategy mode)
3. **`config/levels.json`**: Levels-specific configuration (for levels mode)

> **⚠️ Important**: The `config/` directory is ignored by git (contains API keys and sensitive data). You must create this directory manually.

#### Initial Setup

To set up the configuration:

1. **Create the config directory**:
   ```bash
   mkdir -p config
   ```

2. **Copy the sample configuration files** from `sample_config/`:
   ```bash
   # For Binance
   cp sample_config/binance/bot.json config/bot.json
   cp sample_config/binance/strategies.json config/strategies.json
   cp sample_config/binance/levels.json config/levels.json
   
   # Or for Bybit
   cp sample_config/bybit/bot.json config/bot.json
   cp sample_config/bybit/strategies.json config/strategies.json
   cp sample_config/bybit/levels.json config/levels.json
   ```

3. **Edit `config/bot.json`** and add your:
   - Exchange API keys (see [Configuration](#configuration) for structure)
   - MongoDB connection string if different from default

4. **Customize** `config/strategies.json` and `config/levels.json` as needed for your trading strategy.

> **Note**: The `sample_config/` directory contains template files without API keys. Always use these as a starting point, then add your own credentials to the `config/` directory.

### Main Configuration (`config/bot.json`)

**Binance Configuration**:
```json
{
  "system": {
    "log_level": "info"
  },
  "exchange_engine": {
    "exchange": "Binance",
    "config": {
      "pairs": ["ETH/USDC", "BTC/USDT"],
      "apiKey": "your_api_key",
      "secret": "your_secret_key"
    }
  },
  "mongo": {
    "enabled": true,
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system"
  },
```

**Bybit Configuration** (for futures trading):
```json
{
  "system": {
    "log_level": "info"
  },
  "exchange_engine": {
    "exchange": "Bybit",
    "config": {
      "pairs": ["ETH/USDC", "BTC/USDT"],
      "instrumentType": "Futures",
      "credentials": {
        "apiKey": "your_api_key",
        "apiSecret": "your_secret_key"
      }
    }
  },
  "server": {
    "grpc": {
      "bind_address": "0.0.0.0",
      "port": 50051
    },
    "health": {
      "bind_address": "0.0.0.0",
      "port": 8080
    }
  },
  "channels": {
    "market_data_capacity": 1000,
    "exchange_update_capacity": 1000,
    "indicator_update_capacity": 1000
  },
  "reconciliation": {
    "enabled": true,
    "exchange_level_interval_ms": 180000,
    "trading_level_interval_ms": 180000
  },
  "risk": {
    "total_max_position": null,
    "max_total_orders": null,
    "daily_loss_limit_all_pairs": null,
    "daily_loss_limit_per_pair": {}
  }
}
```

### Levels Configuration (`config/levels.json`)

```json
{
  "levels": [
    {
      "id": "level_1",
      "name": "BTC Buy Level 1",
      "symbol": {
        "base": "BTC",
        "quote": "USDT"
      },
      "side": "Buy",
      "order_type": "Limit",
      "price": {
        "type": "Fixed",
        "value": 50000
      },
      "quantity": {
        "type": "Fixed",
        "value": 0.001
      },
      "risk": {
        "daily_loss_limit": null,
        "max_position": null
      },
      "lifecycle": {
        "auto_repost": true,
        "auto_repost_on_cancel": false
      },
      "enabled": true
    }
  ]
}
```

### Strategies Configuration (`config/strategies.json`)

```json
{
  "strategy_type": "BuyLowSellHigh",
  "symbol": {
    "base": "BTC",
    "quote": "USDT"
  },
  "position_size": 0.001,
  "max_position": 0.01,
  "buy_threshold_percent": 2.0,
  "sell_threshold_percent": 2.0,
  "stop_loss_percent": 5.0,
  "take_profit_percent": 10.0,
  "max_active_orders": 5,
  "price_history_size": 100,
  "min_price_history": 20
}
```

**Configuration Parameters**:
- `strategy_type`: Strategy name (currently "BuyLowSellHigh")
- `symbol`: Trading pair (base and quote currencies)
- `position_size`: Base position size per trade
- `max_position`: Maximum total position size
- `buy_threshold_percent`: Percentage below average price to trigger buy
- `sell_threshold_percent`: Percentage above average price to trigger sell
- `stop_loss_percent`: Stop loss percentage (optional)
- `take_profit_percent`: Take profit percentage (optional)
- `max_active_orders`: Maximum number of active orders
- `price_history_size`: Size of price history buffer
- `min_price_history`: Minimum price history required before trading

For detailed strategy documentation, see [Strategy Guide](docs/STRATEGY_GUIDE.md).

### Environment Variables (Optional)

API keys can be provided via environment variables for security:

```bash
export BINANCE_API_KEY=your_api_key
export BINANCE_API_SECRET=your_secret_key
export MONGODB_URI=mongodb://localhost:27017
```

## Running the System

### Pre-Flight Checks

Before starting the system, run the dependency check script:

```bash
./scripts/check_dependencies.sh
```

This script will:
- Check MongoDB connection
- Check Redis (if enabled)
- Verify ports are free (gRPC: 50051, Health: 8080)
- Auto-start missing dependencies in new terminal windows

### Running in Levels Mode

**Levels mode** is the default mode and supports level-based trading with indicator resolution.

#### 1. Start the Trading System

```bash
# Default (levels mode)
cargo run --release --package runner

# Explicit levels mode
cargo run --release --package runner -- --mode levels

# With custom levels config
cargo run --release --package runner -- --mode levels --levels-config config/levels.json
```

#### 2. Start the TUI CLI (Optional)

In a **separate terminal**, start the TUI CLI:

```bash
# Connect to default gRPC endpoint
cargo run --release --package cli

# Or specify custom endpoint
cargo run --release --package cli -- http://localhost:50051
```

#### 3. Using the TUI CLI

Once the TUI starts, you'll see:

- **Main Menu**: Trading status and navigation
- **Spot Monitor** or **Futures Monitor**: Real-time display (automatically selected based on instrument type)
  - Spot Monitor: Orders, balances, levels/strategies, indicators, trades
  - Futures Monitor: Positions, margin info, orders, orderbook, indicators, trades

**Navigation**:
- `↑/↓`: Navigate levels
- `T`: Toggle level enabled/disabled
- `L`: View level details
- `E`: Edit level
- `C`: Create new level
- `Tab`: Switch between panels
- `Esc`: Close popup/form
- `Q`: Quit

### Running in Strategies Mode

**Strategies mode** supports algorithmic trading strategies (e.g., Buy Low, Sell High).

#### 1. Start the Trading System

```bash
# Strategies mode
cargo run --release --bin runner -- --mode strategies

# With custom strategies config
cargo run --release --bin runner -- --mode strategies --strategies-config config/strategies.json
```

#### 2. Start the TUI CLI (Optional)

In a **separate terminal**, start the TUI CLI:

```bash
# Connect to default gRPC endpoint
cargo run --release --package cli

# Or specify custom endpoint
cargo run --release --package cli -- http://localhost:50051
```

#### 3. Using the TUI CLI in Strategies Mode

Once the TUI starts, you'll see:

- **Main Menu**: Trading status and navigation
- **Spot Monitor** or **Futures Monitor**: Real-time display with **Strategy panel** (replaces Levels panel in strategies mode)
  - Strategy status per strategy (Running/Paused, enabled/disabled)
  - Current position and average price per strategy
  - Unrealized P&L per strategy
  - Active orders count per strategy
  - Last signal per strategy
  - Strategy selection (↑/↓ keys to navigate between strategies)

**Navigation**:
- `T`: Toggle selected strategy on/off (in strategies mode) or toggle selected level (in levels mode)
- `R`: Refresh data
- `Esc`: Back to main menu
- `Q`: Quit

**Trading Control**:
- **Strategies Mode**: Press `T` to toggle the selected strategy (enable/disable individual strategy)
  - Each strategy can be enabled/disabled independently
  - When disabled, the strategy continues monitoring but won't place orders
  - Strategy panel shows "●" (green) when enabled, "⏸" (red) when disabled for each strategy
  - Disabling a strategy automatically cancels its active orders
- **Levels Mode**: Press `T` to toggle the selected level (enable/disable individual level)

### Running Test Binary

The system includes a test binary for full system testing in no-trade mode:

```bash
# Run in levels mode for 60 seconds with verbose logging
cargo run --bin test_runner -- --mode levels --duration 60 --verbose

# Run in strategies mode
cargo run --bin test_runner -- --mode strategies --duration 30
```

**Use Cases**:
- Integration testing
- System validation
- Debugging without placing real orders

### Command-Line Arguments

#### Runner Arguments

```bash
cargo run --package runner -- \
  --mode <strategies|levels> \      # Trading mode (default: levels)
  --strategies-config <path> \       # Strategies config path (default: config/strategies.json)
  --levels-config <path>            # Levels config path (default: config/levels.json)
```

#### TUI CLI Arguments

```bash
cargo run --package cli -- <endpoint>  # gRPC endpoint (default: http://localhost:50051)
```

### Graceful Shutdown

Press `Ctrl+C` to initiate graceful shutdown:
- All components receive shutdown signal
- Active orders are handled
- Connections are closed cleanly
- Timeout: 30 seconds (configurable in config)

## Testing

### Unit Tests

Run all unit tests:

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test --package trading_engine

# Run with output
cargo test -- --nocapture
```

### Integration Tests

The system includes comprehensive integration tests organized in a modular structure:

```bash
# Run all integration tests
cargo test --test integration

# Run specific integration test module
cargo test --test integration levels

# Run ignored tests (requires external services)
cargo test --test integration -- --ignored
```

#### Integration Test Structure

Tests are organized in `tests/integration/`:

- **`config.rs`**: Configuration loading and validation
- **`levels.rs`**: Level management system
- **`risk.rs`**: Risk management system
- **`trading_state.rs`**: Trading state management
- **`circuit_breaker.rs`**: Circuit breaker functionality
- **`market_data.rs`**: Market data engine
- **`indicator.rs`**: Indicator engine
- **`asset.rs`**: Asset engine
- **`exchange.rs`**: Exchange engine (with `#[ignore]` for external calls)
- **`live_config.rs`**: Live config service
- **`level_execution.rs`**: Level execution and resolver
- **`workflows.rs`**: End-to-end workflows
- **`risk_metrics.rs`**: Risk metrics calculation
- **`channels.rs`**: Inter-service channel communication

#### Test Helpers

Common test utilities are in `tests/integration/helpers.rs`:
- `create_test_level_config()` - Create test level configuration
- `create_test_risk_manager()` - Create test risk manager
- `create_test_order()` - Create test order
- `create_test_balance()` - Create test balance
- `create_test_exchange_state_manager()` - Create test exchange state manager
- `create_test_market_data_manager()` - Create test market data manager

#### Ignored Tests

Tests that require external calls (exchange API, network) are marked with `#[ignore]`:

```bash
# Run ignored tests (requires external services)
cargo test --test integration -- --ignored
```

These tests include:
- Exchange engine trading calls
- WebSocket connections
- External API calls

### Test Coverage

The system includes comprehensive test coverage:

- **Unit Tests**: All services have unit tests
- **Integration Tests**: Modular integration test suite
- **Test Binary**: Full system testing in no-trade mode

### Running Tests in CI/CD

```bash
# Run all tests (excluding ignored)
cargo test --test integration

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --tests --test integration
```

## CI/CD

The project includes a GitHub Actions CI pipeline that runs automatically on every commit and pull request.

### GitHub Actions Workflow

The CI pipeline (`.github/workflows/ci.yml`) includes:

1. **Code Formatting Check** (`fmt`)
   - Verifies code follows Rust formatting standards
   - Uses `cargo fmt --check`

2. **Linting** (`clippy`)
   - Runs Clippy with all warnings as errors
   - Catches common Rust issues and anti-patterns

3. **Build Verification** (`build`)
   - Builds all packages (runner, trading-cli)
   - Tests both modes (levels, strategies)
   - Ensures code compiles successfully

4. **Unit Tests** (`test`)
   - Runs all unit tests
   - Fast feedback on core functionality

5. **Integration Tests** (`integration-test`)
   - Runs integration tests (excluding ignored tests)
   - Validates service interactions
   - Excludes external API calls

6. **Cargo Check** (`check`)
   - Verifies all packages compile
   - Catches compilation errors early

7. **Docker Build** (`docker-build`)
   - Validates Docker images build successfully
   - Tests multi-stage builds
   - Uses Docker layer caching

### Running CI Locally

You can run CI checks locally:

```bash
# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests (excluding ignored)
cargo test --test integration

# Build all packages
cargo build --release --package runner
cargo build --release --bin trading-cli

# Check all packages
cargo check --all-targets --all-features
```

### CI Configuration

The CI workflow is configured to:
- Run on all branches (`branches: ['*']`)
- Run on push and pull request events
- Cache Cargo dependencies for faster builds
- Use stable Rust toolchain
- Run tests in parallel where possible

## Maintenance

### Logging

The system uses structured logging via `tracing`:

**Log Levels**:
- `trace`: Very detailed debugging
- `debug`: Debugging information
- `info`: General information (default)
- `warn`: Warning messages
- `error`: Error messages

**Configuration**: Set log level in `config/bot.json`:
```json
{
  "system": {
    "log_level": "info"
  }
}
```

**TUI CLI Logging**: TUI CLI logs to `cli_debug.log` (doesn't clutter terminal)

### Monitoring

#### Health Endpoints

The system exposes health endpoints on port 8080 (default):

```bash
# Health check
curl http://localhost:8080/health

# Trading state
curl http://localhost:8080/state

# Positions (if available)
curl http://localhost:8080/positions
```

#### Distributed Tracing

The system includes comprehensive distributed tracing via the `trading_trace` library:

- **Hot Path Tracing**: Ultra-lightweight tracing for high-frequency operations (market data, order updates)
- **Cold Path Tracing**: Full OpenTelemetry integration for commands, events, and state management
- **Jaeger Integration**: Export traces to Jaeger for visualization
- **Prometheus Metrics**: Pre-configured metrics for trading operations

**See**: [Trading Trace Library README](libs/trading_trace/README.md) for setup, configuration, and usage.

#### Observability stack (Jaeger + Grafana)

To run Jaeger, Prometheus, and Grafana alongside the trading system:

1. **Start the trading system and observability stack** (Docker). Use the rebuild script so the app sends traces to Jaeger:
   ```bash
   ./scripts/rebuild.sh --observability
   ```
   Or manually: `docker-compose -f docker-compose.yml -f docker-compose.observability.yml up -d`.  
   The observability compose file sets `TRACE_JAEGER_AGENT_ENDPOINT=jaeger:6831` on the trading-system so it can reach the Jaeger agent inside Docker.

2. **See traces in Jaeger**: Open http://localhost:16686 → **Search**. Choose service **trading-system** and click **Find Traces**. Traces appear when the app does work (e.g. exchange updates, level/strategy events). If you see no traces, ensure the stack was started with the observability compose file (so the endpoint is set) and that the trading system is running and connected to an exchange.

3. **See traces in Grafana**: Open http://localhost:3000 (login `admin` / `admin`) → **Explore** (compass icon) → select **Jaeger** as data source → choose service **trading-system** and run a query.

4. **Prometheus**: http://localhost:9090 — scrapes `GET /metrics` from the app. The app exposes `trading_system_info`, `trading_system_start_timestamp_seconds`, etc.

5. **See metrics in Grafana**: Open http://localhost:3000 → **Explore** → select **Prometheus** → run a query, e.g. `trading_system_info` or `time() - trading_system_start_timestamp_seconds` (uptime in seconds). To build a dashboard: **Dashboards** → **New** → **Add visualization** → choose Prometheus and a metric.

Config: `observability/prometheus.yml`, `observability/grafana/provisioning/datasources/datasources.yml`.

#### gRPC Monitoring

Use the TUI CLI for real-time monitoring:
- Real-time data streaming
- Level management
- Trading control

### Backup and Recovery

#### MongoDB Backup

```bash
# Backup MongoDB
mongodump --uri="mongodb://localhost:27017" --db=trading_system --out=/backup/path

# Restore MongoDB
mongorestore --uri="mongodb://localhost:27017" --db=trading_system /backup/path/trading_system
```

**Note**: MongoDB stores configuration only, not trading state. State is in-memory.

### Updates and Upgrades

1. **Backup Configuration**: Backup `config/` directory and MongoDB
2. **Pull Latest Code**: `git pull`
3. **Rebuild**: 
   - Local: `cargo build --release`
   - Docker: `./scripts/rebuild.sh` (or `./scripts/rebuild.sh --no-cache` for clean build)
4. **Test**: Run integration tests
5. **Deploy**: Restart the system

### Performance Tuning

#### Channel Buffer Sizes

Adjust channel buffer sizes in `config/bot.json`:

```json
{
  "channels": {
    "market_data_capacity": 1000,
    "exchange_update_capacity": 1000,
    "indicator_update_capacity": 1000
  }
}
```

#### Reconciliation Intervals

Adjust reconciliation intervals:

```json
{
  "reconciliation": {
    "exchange_level_interval_ms": 180000,  # 3 minutes
    "trading_level_interval_ms": 180000    # 3 minutes
  }
}
```

## Troubleshooting

### Common Issues

#### MongoDB Connection Error

```
Error: MongoDB connection failed
```

**Solution**:
1. Check MongoDB is running: `mongosh --eval "db.adminCommand('ping')"`
2. Verify connection string in `config/bot.json`
3. Check firewall rules
4. Check MongoDB logs

#### gRPC Port Already in Use

```
Error: Address already in use (os error 98)
```

**Solution**:
```bash
# Kill process using port 50051
./scripts/kill_grpc_port.sh

# Or change port in config/bot.json
```

#### Exchange API Error

```
Error: Authentication failed
```

**Solution**:
1. Verify API keys in `config/bot.json`
2. Check API key permissions
3. Verify testnet/production environment
4. Check rate limits

#### Rate Limit Errors (429)

```
Error: Rate limited (429) - Request failed
```

**Solution**:
1. The system automatically handles 429 errors with `retryAfter` waiting
2. Check logs for rate limit usage and patterns
3. Monitor `REQUEST_WEIGHT` and `ORDERS` limits
4. Reduce request frequency if consistently hitting limits
5. Check connection count (Binance: 300 connections per 5 minutes per IP)

#### WebSocket Connection Error

```
Error: Trading WebSocket error
```

**Solution**:
1. Check internet connection
2. Verify Binance API is accessible
3. Check firewall rules
4. Verify API keys are valid

#### TUI CLI Connection Error

```
Error: Failed to connect to gRPC server
```

**Solution**:
1. Ensure runner is running first
2. Check gRPC server started (look for "gRPC server started" in logs)
3. Verify endpoint: `http://localhost:50051`
4. Check firewall rules

#### No Trading Activity

**Possible Causes**:
- Levels disabled (check TUI or MongoDB)
- Risk limits reached
- No market data (check WebSocket connection)
- Price conditions not met (for strategies)

**Debugging**:
1. Check logs for errors
2. Verify levels are enabled in TUI
3. Check risk limits in config
4. Monitor market data flow

### Debug Mode

Run in debug mode for more detailed logging:

```bash
# Set log level to debug in config/bot.json
{
  "system": {
    "log_level": "debug"
  }
}

# Or run with RUST_LOG
RUST_LOG=debug cargo run --package runner
```

### Getting Help

1. Check logs: Look for error messages
2. Check health endpoints: Verify system status
3. Review configuration: Ensure all settings are correct
4. Check architecture docs: Understand system behavior

## Architecture Documentation

### Visual Architecture Diagrams

- **[Structurizr Architecture Diagram](architecture/STRUCTURIZR.md)**: Interactive architecture diagrams using Structurizr DSL
  - System Context view
  - Container view
  - Component views for each service
  - Deployment view
  - See [STRUCTURIZR.md](architecture/STRUCTURIZR.md) for usage instructions

### Detailed Architecture Documents

For detailed architecture information, see the [architecture/](architecture/) directory:

- **[Exchange Engine](architecture/01-exchange-engine.md)**: Exchange integration and state management
- **[Market Data Engine](architecture/02-market-data-engine.md)**: Market data streaming
- **[Trading Engine](architecture/03-trading-engine.md)**: Trading logic and execution
- **[Indicator Engine](architecture/04-indicator-engine.md)**: Technical indicator calculation
- **[Risk Engine](architecture/05-risk-engine.md)**: Risk management
- **[Asset Engine](architecture/06-asset-engine.md)**: Asset metadata caching

### Core Libraries

- **[Trading Trace Library](libs/trading_trace/README.md)**: High-performance distributed tracing and observability
  - Hybrid tracing approach (hot path <1μs, cold path <10μs)
  - OpenTelemetry integration with Jaeger and Prometheus exporters
  - Correlation IDs and metrics support
  - See [Trading Trace Library README](libs/trading_trace/README.md) for complete documentation
- **[Live Config Service](architecture/07-live-config-service.md)**: Runtime configuration updates
- **[Runner](architecture/08-runner.md)**: Application orchestrator
- **[TUI CLI](architecture/09-tui-cli.md)**: Terminal user interface
- **[Architecture README](architecture/README.md)**: Architecture overview and principles

For complete system architecture, see [proposals/00-ARCHITECTURE-SUMMARY.md](proposals/00-ARCHITECTURE-SUMMARY.md).

## Authors

**Trading System Contributors** (2024)

For a complete list of contributors, see the [LICENSE](LICENSE) file.

## License

This project is licensed under the **MIT License**.

Copyright © 2024 Trading System Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

See the [LICENSE](LICENSE) file for the full license text.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Ensure all tests pass: `cargo test`
6. Run clippy: `cargo clippy`
7. Submit a pull request

## Version History

- **0.1** (December 2024): Initial release
  - Level-based trading
  - Strategy-based trading
  - TUI CLI
  - Comprehensive testing
  - Architecture documentation
