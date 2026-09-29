# Runner Architecture

**Version**: 0.1  
**Last Updated**: 2026-09-29

The Runner is the application orchestrator that initializes services, wires them together, and exposes gRPC via the gateway crate. Zenoh event/command export is **opt-in** (`ZENOH_ENABLED=1`); leave it unset unless you need that data plane. Candidate #2 uses `btc_price_ma_app`, not the default `runner` binary. Current architecture: [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md).

## Key Principles

- **Orchestration only**: Runner does not contain strategy-model logic; Candidate #2 is injected by `btc_price_ma_app`.
- **Single entry point in lib**: The main entry point is `runner::run()` in the library; it creates `ShutdownManager`, builds all services, and calls `strategy_runner()` or `level_runner()`.
- **Thin binary**: The `runner` binary (`main.rs`) only parses args, loads config, initializes logging/tracing and MongoDB, then calls `runner::run()`.
- **gRPC from gateway**: All gRPC (proto, server, service handlers) lives in the **gateway** crate; the runner depends on `gateway` and spawns the gRPC server in-process.
- **Zenoh opt-in**: Event gateway and command queryable start only when `ZENOH_ENABLED=1` (see [docs/PLATFORM_DATA_PLANE.md](../docs/PLATFORM_DATA_PLANE.md)).

## Architecture

### Structure

```
src/runner/
├── src/
│   ├── main.rs                   # Thin entry: config, logging, tracing, MongoDB, then runner::run()
│   ├── lib.rs                     # run(), strategy_runner(), level_runner(), init helpers, Zenoh gateway wiring
│   ├── config.rs                  # Configuration loading
│   ├── shutdown.rs                # ShutdownManager
│   └── bin/
│       └── test_runner.rs         # Test binary (full init, then spawns runner, runs for duration)
```

gRPC implementation is in **src/gateway/** (see gateway crate):

```
src/gateway/
├── src/
│   ├── main.rs                    # Standalone gRPC proxy binary (optional)
│   ├── lib.rs                     # Re-exports grpc
│   └── grpc/
│       ├── mod.rs
│       ├── proto.rs               # Generated from .proto
│       ├── server.rs              # TradingGrpcServer
│       ├── proxy.rs               # GatewayService (for standalone proxy)
│       └── service/               # TradingCliServiceImpl and RPC handlers
│           ├── mod.rs
│           ├── monitoring.rs
│           ├── levels.rs
│           ├── trading_control.rs
│           └── converters.rs
```

### Components

#### 1. Main entry point (`runner::run()` in lib.rs)

**Purpose**: Single place that constructs `ShutdownManager`, builds all services, and runs the chosen mode.

**Signature** (simplified):

- `run(bot_config, mode, strategies_config, levels_config, mongo_client, tracing_handles) -> Result<()>`

**Responsibilities**:

- Create `ShutdownManager::new()` and call `setup_shutdown_handler()`
- Initialize asset service, fetch asset data
- Initialize market data engine, indicator service, exchange engine
- Create `TradingStateManager`, call `start_trading_state_subscriber()`
- Build `SharedServices`
- Call `strategy_runner()` or `level_runner()` based on `mode`

#### 2. Binary entry point (`main.rs`)

**Purpose**: Thin wrapper that prepares environment and calls the lib.

**Responsibilities**:

- Parse command-line arguments (`--mode`, `--strategies-config`, `--levels-config`)
- Load and validate `config/bot.json`
- Initialize logging and tracing (including Jaeger if configured)
- Call `initialize_mongodb()`
- Call `runner::run(bot_config, mode, strategies_config, levels_config, mongo_client, tracing_handles)`

#### 3. Helper functions (lib, used by `run()` and by `test_runner`)

- **`create_shutdown_manager(capacity)`** – Returns `ShutdownManager` (used by test_runner).
- **`start_trading_state_subscriber(trading_state_manager, shutdown_manager)`** – Spawns subscriber task.
- **`create_strategy_persistence(mongo_client, bot_config)`** – Strategy persistence for MongoDB.
- **`create_level_persistence(mongo_client, bot_config)`** – Level persistence for MongoDB.
- **`setup_shutdown_handler(shutdown_manager)`** – Registers Ctrl-C to signal shutdown.
- **`initialize_mongodb`**, **`initialize_asset_service`**, **`fetch_asset_data`**, **`initialize_market_data_engine`**, **`initialize_indicator_service`**, **`initialize_exchange_engine`** – Service bootstrap.

#### 4. Shutdown management (`shutdown.rs`)

**Purpose**: Graceful shutdown coordination.

**Components**:

- **`ShutdownManager`**: Broadcast channel for shutdown signals; created in `runner::run()` (or via `create_shutdown_manager()` in test_runner).
- **Methods**: `new()`, `signal_shutdown()`, `subscribe()`, `wait_for_shutdown()`

#### 5. gRPC (gateway crate)

**Purpose**: All gRPC surface is in the **gateway** crate; runner uses `gateway::grpc` and spawns the server.

**Flow**:

- Runner builds `TradingCliServiceImpl` (from gateway) with references to its managers (level manager, strategy manager, trading state, exchange state, etc.).
- Runner creates `TradingGrpcServer` (from gateway) and calls `start(addr)` to bind and serve.
- TUI CLI (or any gRPC client) connects to the runner’s gRPC port (default 50051).
- **Optional**: Run the **gateway** binary as a standalone process; it acts as a gRPC proxy forwarding to a runner (e.g. `http://runner:50051`).

#### 6. Zenoh gateway (in runner, using `trading_networking`)

**Purpose**: External event fanout and command handling over Zenoh.

**Flow**:

- Runner creates hot/cold event channels and a command router with `RunnerCommandHandler`.
- Bridges internal engine updates (market data, exchange updates) into `HotEvent` / `ColdEvent` and sends them to the event buses.
- Builds `EventGateway` with `ZenohEventTransport`, subscribes it to the hot/cold receivers, and spawns `gateway.run()`.
- Spawns `serve_zenoh_commands(session, cmd_key, router)` so Zenoh clients can send commands (e.g. set trading enabled, get status, toggle strategy).
- Zenoh is always enabled (no env flag).

#### 7. Test binary (`bin/test_runner.rs`)

**Purpose**: Full system test without placing real orders.

**Features**:

- Uses the same init helpers as production (e.g. `create_shutdown_manager`, `start_trading_state_subscriber`, `create_*_persistence`).
- Builds `SharedServices` and spawns `strategy_runner()` or `level_runner()` in a task.
- Runs for a configurable duration (or until Ctrl-C), then signals shutdown and waits for the runner task.

**Usage**:

```bash
cargo run --bin test_runner -- --mode levels --duration 60 --verbose
```

## Data Flow

### Application startup (production)

```
main.rs
    ├── Parse arguments, load config
    ├── Initialize logging and tracing
    ├── initialize_mongodb()
    └── runner::run(...)
            ├── ShutdownManager::new(), setup_shutdown_handler()
            ├── initialize_asset_service, fetch_asset_data
            ├── initialize_market_data_engine
            ├── initialize_indicator_service
            ├── initialize_exchange_engine
            ├── TradingStateManager, start_trading_state_subscriber()
            ├── SharedServices
            ├── strategy_runner() or level_runner()
            │       ├── Start gRPC server (gateway::grpc)
            │       ├── Wire Zenoh event gateway + command queryable
            │       └── Run event loops until shutdown
            └── trading_trace::shutdown_exporters()
```

## Communication

### Internal

- **Channels**: All services communicate via broadcast/mpsc channels.
- **SharedServices**: Common services passed into `strategy_runner` / `level_runner`.

### External

- **gRPC**: Provided by the gateway crate; runner spawns the server. `trading-system-ui` (gRPC-Web via Envoy) and the legacy TUI are clients.
- **Zenoh**: Opt-in (`ZENOH_ENABLED=1`). Events (hot/cold) published; commands via queryable. Not required for Paper or Demo labs.
- **MongoDB**: Configuration persistence.

## Configuration

- **Channel capacities**: In `config/bot.json` under `channels` (e.g. `market_data_capacity`, `exchange_update_capacity`, `shutdown_capacity`).
- **gRPC / health**: In `config/bot.json` under `server` (grpc bind/port, health bind/port).
- **Zenoh**: Uses default session; keyspace can be derived from bot config (e.g. `/trading/{bot_id}/...`).

## Dependencies

- **gateway**: gRPC proto, server, service implementation.
- **trading_networking** (with Zenoh feature): Events, commands, event gateway, Zenoh transport.
- All engine crates (exchange_engine, market_data_engine, trading_engine, indicator_engine, asset_engine, live_config).
- **trading_core**, **trading_trace**, **tokio**, **tracing**, etc.

## Testing

- **test_runner** binary: Full system run with configurable duration; no real orders.
- Integration tests: Service initialization, gRPC endpoints, channel behavior.

## Error handling and shutdown

- Configuration and MongoDB errors cause early exit.
- Ctrl-C triggers `ShutdownManager::signal_shutdown()`; runners exit their loops and the process shuts down.
- gRPC and Zenoh tasks are spawned; their errors are logged.

## Security

- API keys and secrets from config (e.g. `SecretString`).
- gRPC and Zenoh can be extended with authentication if required.
