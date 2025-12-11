# Runner Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Runner is the application orchestrator that initializes all services, wires them together, and provides the gRPC server for external communication (TUI CLI). It supports both strategy-based and level-based trading modes.

## Key Principle

Runner is the **orchestration layer** - it doesn't contain business logic, but coordinates all services and provides the entry point for the application.

## Architecture

### Structure

```
src/runner/
├── src/
│   ├── main.rs                   # Application entry point
│   ├── lib.rs                     # Core orchestration logic
│   ├── config.rs                  # Configuration loading
│   ├── shutdown.rs                # Graceful shutdown management
│   ├── grpc/
│   │   ├── service/
│   │   │   ├── mod.rs            # Service orchestrator
│   │   │   ├── monitoring.rs     # Monitoring RPC handlers
│   │   │   ├── levels.rs          # Level management RPC handlers
│   │   │   ├── trading_control.rs # Trading control RPC handlers
│   │   │   └── converters.rs     # Type converters
│   │   └── server.rs              # gRPC server setup
│   └── bin/
│       └── test_runner.rs         # Test binary for full system testing
```

### Components

#### 1. Main Entry Point (`main.rs`)

**Purpose**: Application entry point that parses arguments and routes to appropriate runner.

**Responsibilities**:
- Parse command-line arguments (`--mode strategy|levels`)
- Load configuration files
- Initialize MongoDB (if enabled)
- Initialize shared services
- Route to `strategy_runner()` or `level_runner()`
- Start gRPC server (if enabled)

#### 2. Core Orchestration (`lib.rs`)

**Purpose**: Core orchestration logic for service initialization and coordination.

**Key Functions**:

##### Service Initialization

- `initialize_mongodb(config: &BotConfig) -> Option<MongoClient>` - Initialize MongoDB
- `initialize_asset_service() -> AssetService` - Initialize asset engine
- `fetch_asset_data(asset_service: &AssetService, config: &BotConfig)` - Fetch asset data at bootstrap
- `initialize_market_data_engine(config: &BotConfig, channels: &ChannelConfig) -> MarketDataDriver` - Initialize market data engine
- `initialize_indicator_service(config: &BotConfig, indicator_tx: IndicatorUpdateSender) -> IndicatorService` - Initialize indicator engine
- `initialize_exchange_engine(config: &BotConfig, channels: &ChannelConfig) -> ExchangeStateManager` - Initialize exchange engine
- `collect_configured_pairs(config: &BotConfig) -> Vec<Pair>` - Collect pairs from config

##### Runners

- `strategy_runner(config: BotConfig, services: SharedServices) -> Result<()>` - Strategy mode setup
- `level_runner(config: BotConfig, services: SharedServices) -> Result<()>` - Levels mode setup

##### Shared Services

- `SharedServices` struct encapsulates common services:
  - Trading client
  - Execution report channel
  - Shutdown manager
  - Market data handle
  - Trading WebSocket handle
  - Indicator handle
  - Indicator receiver (`indicator_rx`) for shared indicator channel

##### Execution Report Handler

- `execution_report_handler()` - Shared handler function eliminates duplication
  - Common logic for receiving and processing execution reports
  - Mode-specific callbacks for strategy vs levels handling

#### 3. Configuration (`config.rs`)

**Purpose**: Configuration loading and validation.

**Configuration Files**:
- `config/bot.json`: General bot config (system, exchange, server, redis, features, channels)
- `config/strategies.json`: Strategy-specific config
- `config/levels.json`: Levels-specific config

**Key Structures**:
- `BotConfig`: Main bot configuration
- `StrategiesConfig`: Strategy-specific configuration
- `LevelsConfig`: Levels-specific configuration (loaded from MongoDB or file)

**Methods**:
- `BotConfig::load_from_file(path: &str) -> Result<BotConfig>` - Load bot config
- `BotConfig::validate() -> Result<()>` - Validate bot config
- `StrategiesConfig::load_from_file(path: &str) -> Result<StrategiesConfig>` - Load strategies config
- `LevelsConfig::load_from_mongo(mongo: &MongoClient) -> Result<LevelsConfig>` - Load levels from MongoDB

#### 4. Shutdown Management (`shutdown.rs`)

**Purpose**: Graceful shutdown coordination.

**Components**:
- `ShutdownManager`: Coordinates shutdown signals
  - Broadcast channel for shutdown signals
  - Subscribers can listen for shutdown
  - Supports graceful shutdown of all services

**Methods**:
- `new() -> ShutdownManager` - Create new shutdown manager
- `signal_shutdown()` - Signal shutdown to all subscribers
- `subscribe() -> ShutdownReceiver` - Subscribe to shutdown signals
- `wait_for_shutdown() -> impl Future` - Wait for shutdown signal

#### 5. gRPC Service (`grpc/`)

**Purpose**: gRPC server for external communication (TUI CLI).

##### Service Implementation (`grpc/service/`)

**Structure**:
- `mod.rs`: Service orchestrator, defines `TradingCliServiceImpl`
- `monitoring.rs`: Monitoring RPC handlers
  - `collect_monitoring_data()` - Aggregate monitoring data
  - `get_trades()` - Get recent trades
  - `get_indicators()` - Get indicator values
  - `get_monitoring_data()` - Get all monitoring data
  - `stream_monitoring_data()` - Stream monitoring data updates

- `levels.rs`: Level management RPC handlers
  - `get_levels()` - Get all levels
  - `get_level()` - Get specific level
  - `create_level()` - Create new level
  - `update_level()` - Update level
  - `delete_level()` - Delete level
  - `toggle_level()` - Toggle level enabled/disabled

- `trading_control.rs`: Trading control RPC handlers
  - `set_trading_enabled()` - Enable/disable trading
  - `get_trading_status()` - Get trading status

- `converters.rs`: Type converters
  - Conversion between internal types and protobuf messages
  - `to_proto_level()`, `from_proto_level_config()`, etc.

##### Server Setup (`grpc/server.rs`)

**Purpose**: Sets up and runs gRPC server.

**Responsibilities**:
- Create gRPC server with service implementation
- Bind to configured address (default: 0.0.0.0:50051)
- Handle graceful shutdown

#### 6. Test Binary (`bin/test_runner.rs`)

**Purpose**: Test binary for full system testing in no-trade mode.

**Features**:
- Runs full system without placing actual trades
- Configurable duration and verbosity
- Supports both strategy and levels modes
- Useful for integration testing

**Usage**:
```bash
cargo run --bin test_runner -- --mode levels --duration 60 --verbose
```

## Data Flow

### Application Startup Flow

```
main.rs
    ├── Parse arguments
    ├── Load configuration
    ├── Initialize MongoDB (if enabled)
    ├── Initialize Asset Service
    ├── Fetch asset data (bootstrap)
    ├── Create shared indicator channel
    ├── Initialize Market Data Engine
    ├── Initialize Indicator Service
    ├── Initialize Exchange Engine
    ├── Create SharedServices
    ├── Route to strategy_runner() or level_runner()
    └── Start gRPC server (if enabled)
```

### Service Initialization Flow

```
initialize_*_service()
    ├── Extract exchange config from BotConfig (internally)
    ├── Create service instance
    ├── Configure channels (from ChannelConfig)
    └── Return service handle
```

## Communication

### Internal
- **Channels**: All services communicate via broadcast channels
- **Shared Services**: Common services passed via `SharedServices` struct

### External
- **gRPC**: TUI CLI connects via gRPC
- **MongoDB**: Configuration persistence (optional)

## Configuration

### Channel Configuration

Centralized channel buffer sizes in `ChannelConfig`:
- `market_data_capacity`: 1000 (default)
- `exchange_update_capacity`: 1000 (default)
- `indicator_update_capacity`: 1000 (default)
- `websocket_inbound_capacity`: 1024 (default)
- `websocket_ping_capacity`: 16 (default)
- `websocket_outbound_rpc_capacity`: 1024 (default)
- `live_config_request_capacity`: 100 (default)
- `shutdown_capacity`: 1 (default)
- `market_data_engine_buffer`: 1000 (default)

### Exchange-Agnostic Design

**Key Feature**: All initialization functions handle exchange config internally.

**Benefits**:
- Scalable: Adding new exchanges only requires adding branches in initialization functions
- No exchange-specific code in `main.rs`
- Clean separation of concerns

**Example**:
```rust
// Exchange-agnostic initialization
let market_data_driver = initialize_market_data_engine(&bot_config, &channels)?;
// Function internally extracts Binance config from BotConfig
```

## Dual Mode Support

### Strategy Mode

**Entry Point**: `strategy_runner()`

**Setup**:
1. Load strategies config
2. Initialize strategy executor
3. Start strategy execution loop
4. Handle execution reports

### Levels Mode

**Entry Point**: `level_runner()`

**Setup**:
1. Load levels config (from MongoDB or file)
2. Initialize level manager
3. Initialize level executor
4. Start level event loop
5. Handle execution reports

## Shared Indicator Channel

**Architecture**: Single channel for indicator updates created in `main.rs` before service initialization.

**Flow**:
1. Channel created in `main.rs`
2. Sender passed to `initialize_indicator_service()`
3. Receiver added to `SharedServices`
4. Runners use `services.indicator_rx`

**Benefit**: Prevents channel closure errors by ensuring receiver stays alive throughout application lifetime.

## gRPC Server

### Endpoints

**Monitoring**:
- `GetMonitoringData` - Get current monitoring data
- `StreamMonitoringData` - Stream real-time updates

**Level Management**:
- `GetLevels` - Get all levels
- `GetLevel` - Get specific level
- `CreateLevel` - Create new level
- `UpdateLevel` - Update level
- `DeleteLevel` - Delete level
- `ToggleLevel` - Toggle level enabled/disabled

**Trading Control**:
- `SetTradingEnabled` - Enable/disable trading
- `GetTradingStatus` - Get trading status

### Data Aggregation

gRPC service aggregates data from multiple sources:
- `ExchangeStateManager`: Balances, orders, trades
- `IndicatorService`: Indicator values
- `TradingStateManager`: Trading state
- `LevelManager`: Level configurations

## Performance Characteristics

- **Startup Time**: < 5 seconds (with MongoDB)
- **Memory**: Efficient service initialization
- **Channel Setup**: O(1) channel creation

## Dependencies

- All service crates (exchange_engine, market_data_engine, trading_engine, etc.)
- `trading_core`: Types, channels, config
- `tonic`: gRPC server
- `mongodb`: Optional persistence

## Testing

- Integration tests for service initialization
- Integration tests for gRPC endpoints
- Test binary for full system testing

## Error Handling

- **Configuration Errors**: Validated at startup
- **Service Initialization Failures**: Logged, application exits
- **gRPC Errors**: Handled gracefully, logged
- **Shutdown**: Graceful shutdown of all services

## Security

- **API Keys**: Loaded from config, use `SecretString`
- **gRPC**: Can add authentication if needed
- **Configuration**: Validated before use

