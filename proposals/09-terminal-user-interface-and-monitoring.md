# Proposal 09: Terminal User Interface (TUI) and Monitoring

## Overview

This proposal defines a production-ready Terminal User Interface (TUI) CLI for the trading system. The TUI serves as the **primary monitoring and control interface**, providing real-time trading data visualization, level configuration, and system health monitoring. The TUI connects directly to the Trading System via gRPC (running in `runner`), receiving aggregated monitoring data through streaming.

**Key Insight**: The TUI **IS** the monitoring interface - there is no separate monitoring service. The gRPC server in `runner` exposes monitoring data directly to the TUI.

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    TUI CLI Application                   │
│              (External Process - gRPC Client)             │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │   UI Layer   │  │  Client Layer │  │  State Layer  │ │
│  │  (ratatui)   │  │   (gRPC)      │  │   (Local)     │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬────────┘ │
│         │                  │                  │          │
│         └──────────────────┼──────────────────┘          │
│                            │                             │
└────────────────────────────┼─────────────────────────────┘
                             │
                             │ gRPC Streaming
                             │
┌────────────────────────────▼─────────────────────────────┐
│         Trading System (Monolith)                         │
│                                                           │
│  ┌──────────────────────────────────────────────────┐    │
│  │  Runner (gRPC Server)                           │    │
│  │  - Level Manager                                 │    │
│  │  - Trading State                                 │    │
│  │  - Risk Manager                                  │    │
│  │  - Monitoring Data Aggregator                   │    │
│  └──────────────────────────────────────────────────┘    │
│                                                           │
│  ┌──────────────────────────────────────────────────┐    │
│  │  Trading Engine                                  │    │
│  │  - Receives data via channels (internal)        │    │
│  │  - Aggregates monitoring data                    │    │
│  └──────────────────────────────────────────────────┘    │
│                                                           │
│  ┌──────────────────────────────────────────────────┐    │
│  │  Exchange Engine → Channel → Trading Engine     │    │
│  │  Market Data Engine → Channel → Trading Engine  │    │
│  └──────────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────────┘
```

### Communication Flow

**Internal (Monolith)**:
```
Exchange Engine → Channel → Trading Engine
Market Data Engine → Channel → Trading Engine
Indicator Service → Channel → Trading Engine
```

**External (TUI)**:
```
Trading Engine → gRPC Server (in runner) → TUI CLI
```

## Key Requirements

1. **gRPC Server in Runner**: Exposes monitoring data and level management to TUI
2. **Data Aggregation**: Trading Engine aggregates data (per pair, per level, overall)
3. **Real-Time Streaming**: gRPC streaming for live updates
4. **Level Management**: Complete CRUD operations via gRPC
5. **Visual Clarity**: Color-coded displays, markers, intuitive navigation
6. **Performance**: Efficient rendering, minimal latency, smooth updates

## Component Structure

### Trading System Side (Runner + Trading Engine)

```
runner/
├── src/
│   ├── main.rs                     # Orchestrates components, starts gRPC server
│   └── grpc/                       # NEW: gRPC module (separate from main.rs)
│       ├── mod.rs                  # Module exports
│       ├── server.rs               # gRPC server setup and lifecycle
│       ├── service.rs              # TradingCliService implementation
│       ├── handlers/               # Request handlers
│       │   ├── mod.rs
│       │   ├── level_handlers.rs   # Level management RPC handlers
│       │   ├── monitoring_handlers.rs # Monitoring data streaming handlers
│       │   └── health_handlers.rs  # System/protocol health handlers
│       └── types.rs                # gRPC message type conversions

trading_engine/
├── src/
│   ├── monitoring/
│   │   ├── mod.rs
│   │   ├── aggregator.rs          # Aggregates data (per pair, per level, overall)
│   │   └── data_builder.rs        # Builds monitoring data from trading state
│   └── ... (existing trading engine)
```

### TUI CLI Side

```
cli/
├── Cargo.toml
├── src/
│   ├── main.rs                    # Entry point
│   ├── app.rs                     # Main application state
│   ├── client.rs                  # gRPC client
│   ├── screens/
│   │   ├── mod.rs
│   │   ├── main_menu.rs           # Main menu screen
│   │   ├── level_setup/           # Level management screens
│   │   ├── trading_monitor/       # Trading monitoring screens
│   │   └── system_monitor/        # System health screens
│   ├── widgets/                   # Reusable UI widgets
│   ├── formatters/                # Data formatting utilities
│   └── styles/                    # Color schemes and themes
└── proto/                         # gRPC proto files
    └── trading_cli.proto
```

## Detailed Component Design

### 1. gRPC Server in Runner (`runner/src/grpc/server.rs`)

**Purpose**: Exposes monitoring data and level management to TUI via gRPC.

```rust
use tonic::transport::Server;
use trading_engine::monitoring::{MonitoringData, Aggregator};

pub struct TradingGrpcServer {
    level_manager: Arc<RwLock<LevelManager>>,
    trading_state: Arc<RwLock<TradingState>>,
    risk_manager: Arc<RiskManager>,
    aggregator: Arc<Aggregator>,
}

impl TradingGrpcServer {
    pub async fn start(
        &self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let service = TradingCliService::new(
            self.level_manager.clone(),
            self.trading_state.clone(),
            self.risk_manager.clone(),
            self.aggregator.clone(),
        );

        Server::builder()
            .add_service(TradingCliServiceServer::new(service))
            .serve(addr)
            .await?;

        Ok(())
    }
}
```

### 2. Monitoring Data Aggregator (`trading_engine/src/monitoring/aggregator.rs`)

**Purpose**: Aggregates data per pair, per level, and overall for monitoring.

```rust
pub struct Aggregator;

#[derive(Debug, Clone, Serialize)]
pub struct AggregatedData {
    /// Overall statistics
    pub overall: OverallStatistics,
    
    /// Statistics per pair
    pub per_pair: HashMap<Pair, PairStatistics>,
    
    /// Statistics per level
    pub per_level: HashMap<String, LevelStatistics>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverallStatistics {
    pub total_orders: usize,
    pub total_positions: usize,
    pub total_position_value: Decimal,
    pub total_pnl: Decimal,
    pub total_balance: Decimal,
    pub active_pairs: usize,
    pub active_levels: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairStatistics {
    pub pair: Pair,
    pub orders: Vec<Order>,
    pub order_count: usize,
    pub position: Option<Position>,
    pub pnl: Decimal,
    pub trades: Vec<Trade>,
    pub trade_count: usize,
    pub base_balance: Decimal,
    pub quote_balance: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct LevelStatistics {
    pub level_id: String,
    pub orders: Vec<Order>,
    pub order_count: usize,
    pub filled_count_today: usize,
    pub filled_quantity_today: Decimal,
    pub pnl: Decimal,
    pub last_fill_at: Option<u64>,
}

impl Aggregator {
    /// Aggregate all data from trading state
    pub fn aggregate(
        &self,
        state: &TradingState,
        levels: Option<&HashMap<String, Level>>,
    ) -> AggregatedData;
    
    /// Calculate overall statistics
    fn calculate_overall(&self, state: &TradingState) -> OverallStatistics;
    
    /// Calculate per-pair statistics
    fn calculate_per_pair(&self, state: &TradingState) -> HashMap<Pair, PairStatistics>;
    
    /// Calculate per-level statistics
    fn calculate_per_level(
        &self,
        state: &TradingState,
        levels: Option<&HashMap<String, Level>>,
    ) -> HashMap<String, LevelStatistics>;
}
```

### 3. Monitoring Data Builder (`trading_engine/src/monitoring/data_builder.rs`)

**Purpose**: Builds complete monitoring data from trading state.

```rust
pub struct DataBuilder;

#[derive(Debug, Clone, Serialize)]
pub struct MonitoringData {
    /// All open orders
    pub orders: Vec<Order>,
    
    /// All balances
    pub balances: Vec<Balance>,
    
    /// All trades (last 24h)
    pub trades: Vec<Trade>,
    
    /// All positions
    pub positions: Vec<Position>,
    
    /// Aggregated data
    pub aggregated: AggregatedData,
    
    /// Timestamp
    pub timestamp: u64,
}

impl DataBuilder {
    /// Build complete monitoring data from trading state
    pub fn build(
        &self,
        state: &TradingState,
        levels: Option<&HashMap<String, Level>>,
        aggregator: &Aggregator,
    ) -> MonitoringData;
}
```

### 4. gRPC Service Implementation (`runner/src/grpc/service.rs`)

**Purpose**: Implements gRPC service for TUI.

```rust
use tonic::{Request, Response, Status};
use trading_cli_proto::{
    trading_cli_service_server::TradingCliService,
    *,
};

pub struct TradingCliServiceImpl {
    level_manager: Arc<RwLock<LevelManager>>,
    trading_state: Arc<RwLock<TradingState>>,
    risk_manager: Arc<RiskManager>,
    aggregator: Arc<Aggregator>,
    data_builder: Arc<DataBuilder>,
}

#[tonic::async_trait]
impl TradingCliService for TradingCliServiceImpl {
    /// Get all levels
    async fn get_levels(
        &self,
        _request: Request<GetLevelsRequest>,
    ) -> Result<Response<GetLevelsResponse>, Status> {
        let levels = self.level_manager.read().unwrap().get_all_levels();
        Ok(Response::new(GetLevelsResponse {
            levels: levels.into_iter().map(|l| l.into()).collect(),
        }))
    }

    /// Create level
    async fn create_level(
        &self,
        request: Request<CreateLevelRequest>,
    ) -> Result<Response<CreateLevelResponse>, Status> {
        let req = request.into_inner();
        let config = LevelConfig::from_proto(req.config)?;
        
        let level_id = self.level_manager.write().unwrap()
            .add_level(config)?;
        
        Ok(Response::new(CreateLevelResponse { level_id }))
    }

    /// Stream monitoring data
    async fn stream_monitoring_data(
        &self,
        _request: Request<StreamMonitoringDataRequest>,
    ) -> Result<Response<ReceiverStream<MonitoringDataUpdate>>, Status> {
        let (tx, rx) = mpsc::channel(100);
        
        // Spawn task to send updates
        let state = self.trading_state.clone();
        let aggregator = self.aggregator.clone();
        let data_builder = self.data_builder.clone();
        let levels = self.level_manager.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            
            loop {
                interval.tick().await;
                
                let state_guard = state.read().unwrap();
                let levels_guard = levels.read().unwrap();
                
                let monitoring_data = data_builder.build(
                    &state_guard,
                    Some(&levels_guard.get_all_levels_map()),
                    &aggregator,
                );
                
                let update = MonitoringDataUpdate {
                    data: Some(monitoring_data.into()),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                };
                
                if tx.send(update).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// Get system health
    async fn get_system_health(
        &self,
        _request: Request<GetSystemHealthRequest>,
    ) -> Result<Response<GetSystemHealthResponse>, Status> {
        // Build system health from components
        let health = SystemHealth {
            trading_engine: true,
            market_data: true,
            exchange_engine: true,
            level_manager: true,
            risk_system: true,
            uptime_secs: 0, // Calculate from start time
        };
        
        Ok(Response::new(GetSystemHealthResponse { health: Some(health) }))
    }
}
```

### 5. TUI gRPC Client (`cli/src/client.rs`)

**Purpose**: gRPC client for connecting to Trading System.

```rust
use tonic::transport::Channel;
use trading_cli_proto::trading_cli_service_client::TradingCliServiceClient;

pub struct TradingCliClient {
    grpc_client: TradingCliServiceClient<Channel>,
    connection_string: String,
}

impl TradingCliClient {
    /// Connect to trading system
    pub async fn connect(endpoint: &str) -> Result<Self, CliError> {
        let client = TradingCliServiceClient::connect(endpoint.to_string())
            .await?;
        
        Ok(Self {
            grpc_client: client,
            connection_string: endpoint.to_string(),
        })
    }
    
    /// Get all levels
    pub async fn get_levels(&self) -> Result<Vec<Level>, CliError> {
        let request = Request::new(GetLevelsRequest {});
        let response = self.grpc_client.get_levels(request).await?;
        Ok(response.into_inner().levels.into_iter().map(|l| l.into()).collect())
    }
    
    /// Stream monitoring data
    pub async fn stream_monitoring_data(
        &self,
    ) -> Result<Receiver<MonitoringDataUpdate>, CliError> {
        let request = Request::new(StreamMonitoringDataRequest {});
        let mut stream = self.grpc_client.stream_monitoring_data(request).await?
            .into_inner();
        
        let (tx, rx) = mpsc::channel(100);
        
        tokio::spawn(async move {
            while let Some(update) = stream.message().await.unwrap() {
                let _ = tx.send(update).await;
            }
        });
        
        Ok(rx)
    }
}
```

## gRPC Proto Definition

### `trading_cli.proto`

```protobuf
syntax = "proto3";

package trading_cli;

service TradingCliService {
    // Level management
    rpc GetLevels(GetLevelsRequest) returns (GetLevelsResponse);
    rpc CreateLevel(CreateLevelRequest) returns (CreateLevelResponse);
    rpc UpdateLevel(UpdateLevelRequest) returns (UpdateLevelResponse);
    rpc DeleteLevel(DeleteLevelRequest) returns (DeleteLevelResponse);
    rpc ToggleLevel(ToggleLevelRequest) returns (ToggleLevelResponse);
    
    // Monitoring data
    rpc GetMonitoringData(GetMonitoringDataRequest) returns (GetMonitoringDataResponse);
    rpc StreamMonitoringData(StreamMonitoringDataRequest) returns (stream MonitoringDataUpdate);
    
    // System health
    rpc GetSystemHealth(GetSystemHealthRequest) returns (GetSystemHealthResponse);
    rpc GetProtocolHealth(GetProtocolHealthRequest) returns (GetProtocolHealthResponse);
    
    // LiveConfig - Runtime configuration updates
    rpc UpdateLevelValue(UpdateLevelValueRequest) returns (UpdateResponse);
    rpc UpdateRiskLimit(UpdateRiskLimitRequest) returns (UpdateResponse);
    rpc UpdateBotConfig(UpdateBotConfigRequest) returns (UpdateResponse);
    rpc StreamConfigUpdates(StreamConfigUpdatesRequest) returns (stream ConfigUpdateNotification);
}

message MonitoringDataUpdate {
    MonitoringData data = 1;
    int64 timestamp = 2;
}

message MonitoringData {
    repeated Order orders = 1;
    repeated Balance balances = 2;
    repeated Trade trades = 3;
    repeated Position positions = 4;
    AggregatedData aggregated = 5;
    repeated IndicatorValue indicators = 6;  // Current indicator values
    IndicatorMetadata indicator_metadata = 7; // Available indicators and their configs
}

message AggregatedData {
    OverallStatistics overall = 1;
    map<string, PairStatistics> per_pair = 2;
    map<string, LevelStatistics> per_level = 3;
}

message IndicatorValue {
    string indicator_id = 1;
    string indicator_name = 2;
    string pair = 3;
    string value = 4;  // Decimal as string
    int64 timestamp = 5;
    int32 period = 6;
    bool is_composite = 7;  // true if composite indicator
    repeated string dependencies = 8;  // For composite indicators
}

message IndicatorMetadata {
    repeated SimpleIndicatorInfo simple_indicators = 1;
    repeated CompositeIndicatorInfo composite_indicators = 2;
}

message SimpleIndicatorInfo {
    string id = 1;
    string name = 2;  // "SMA", "EMA", etc.
    string pair = 3;
    int32 period = 4;
    string source = 5;  // "Close", "Open", "High", "Low", etc.
}

message CompositeIndicatorInfo {
    string id = 1;
    string name = 2;
    string pair = 3;
    string expression_json = 4;  // JSON representation of expression tree
    repeated string dependencies = 5;
}

// ... other message types
```

## TUI Menu Designs

This section contains detailed ASCII art designs for all TUI screens. These designs serve as the specification for implementation.

### 1. Main Menu

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Trading System Control                                │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Trading Status: [● ENABLED]  [○ DISABLED]                       │  │
│  │  Press [T] to toggle trading on/off                               │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Navigation:                                                      │  │
│  │                                                                   │  │
│  │  [1] Trading Monitor                                             │  │
│  │      View orderbooks, indicators, orders, balances, trades,      │  │
│  │      and manage levels                                           │  │
│  │                                                                   │  │
│  │  [2] System Monitor                                              │  │
│  │      View connectivity status, channel health, and system info   │  │
│  │                                                                   │  │
│  │  [3] Live Config                                                 │  │
│  │      Replace entire config (levels.json or strategies.json)      │  │
│  │      without restart                                             │  │
│  │                                                                   │  │
│  │  [Q] Quit Application                                            │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  Mode: [Levels]  Exchange: [Binance]  Uptime: [2h 15m 32s]            │
└─────────────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
- `1` - Open Trading Monitor
- `2` - Open System Monitor
- `3` - Open Live Config
- `T` - Toggle trading enabled/disabled (cancels all orders when disabling)
- `Q` - Quit application

---

### 2. Trading Monitor (Main Screen)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Trading Monitor                                    [ESC] Back to Menu   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─ Orderbook (BTC/USDT) ───────────────────────────────────────────┐ │
│  │  Best Bid: 45,230.50  │  Best Ask: 45,231.00  │  Spread: 0.50    │ │
│  │  Mid Price: 45,230.75  │  Last Update: 14:23:45                    │ │
│  │                                                                    │ │
│  │  Bids:                    │  Asks:                                 │ │
│  │  45,230.50  (0.5)         │  45,231.00  (0.3)                     │ │
│  │  45,230.00  (1.2)         │  45,231.50  (0.8)                     │ │
│  │  45,229.50  (2.1)         │  45,232.00  (1.5)                     │ │
│  │  ...                      │  ...                                  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Indicators ─────────────────────────────────────────────────────┐ │
│  │  ID              │ Name      │ Value      │ Period │ Status      │ │
│  ├──────────────────┼───────────┼────────────┼────────┼─────────────┤ │
│  │ sma_20_btcusdt   │ SMA       │ 45,180.25  │ 20     │ ✓ Active    │ │
│  │ ema_12_btcusdt   │ EMA       │ 45,195.50  │ 12     │ ✓ Active    │ │
│  │ ema_50_btcusdt   │ EMA       │ 44,950.00  │ 50     │ ✓ Active    │ │
│  │ *golden_cross    │ Golden X  │ 245.50     │ -      │ ✓ Active    │ │
│  │ *macd_btcusdt    │ MACD      │ 125.30     │ -      │ ✓ Active    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Open Orders ────────────────────────────────────────────────────┐ │
│  │  Order ID    │ Side │ Price      │ Quantity │ Status    │ Level  │ │
│  ├──────────────┼──────┼────────────┼──────────┼───────────┼────────┤ │
│  │ #12345678    │ BUY  │ 45,000.00  │ 0.001    │ NEW       │ L1     │ │
│  │ #12345679    │ SELL │ 45,500.00  │ 0.001    │ PARTIAL   │ L2     │ │
│  │ #12345680    │ BUY  │ 44,800.00  │ 0.002    │ FILLED    │ L3     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Balances ────────────────────────────────────────────────────────┐ │
│  │  Asset │ Free      │ Used      │ Locked    │ Total                 │ │
│  ├────────┼───────────┼───────────┼───────────┼──────────────────────┤ │
│  │ BTC    │ 0.500000  │ 0.000000  │ 0.001000  │ 0.501000             │ │
│  │ USDT   │ 1,250.50  │ 45.00     │ 0.00      │ 1,295.50             │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Recent Trades ────────────────────────────────────────────────────┐ │
│  │  Time      │ Order ID │ Side │ Price      │ Quantity │ Status    │ │
│  ├────────────┼──────────┼──────┼────────────┼──────────┼───────────┤ │
│  │ 14:23:45   │ #12345680│ BUY  │ 44,800.00  │ 0.002    │ FILLED    │ │
│  │ 14:20:12   │ #12345675│ SELL │ 45,600.00  │ 0.001    │ FILLED    │ │
│  │ 14:18:33   │ #12345670│ BUY  │ 44,750.00  │ 0.001    │ FILLED    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Levels ───────────────────────────────────────────────────────────┐ │
│  │  Level ID │ Status  │ Price      │ Quantity │ Orders │ [L] Manage│ │
│  ├───────────┼─────────┼────────────┼──────────┼────────┼───────────┤ │
│  │ level_1   │ ACTIVE  │ 45,000.00  │ 0.001    │ 1      │           │ │
│  │ level_2   │ ACTIVE  │ 45,500.00  │ 0.001    │ 1      │           │ │
│  │ level_3   │ PAUSED  │ 44,800.00  │ 0.002    │ 0      │           │ │
│  │ level_4   │ ACTIVE  │ 46,000.00  │ 0.001    │ 0      │           │ │
│  │                                                                    │ │
│  │  Press [L] to open Level Management                                │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  [R] Refresh  [L] Level Management  [ESC] Back                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
- `L` - Open Level Management
- `R` - Refresh data
- `ESC` - Back to Main Menu

---

### 3. Level Management (Accessed from Trading Monitor)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Level Management                              [ESC] Back to Trading Monitor│
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─ All Levels ──────────────────────────────────────────────────────┐ │
│  │  ID      │ Status  │ Enabled │ Price      │ Quantity │ Orders │ Filled│ │
│  ├──────────┼─────────┼─────────┼────────────┼──────────┼────────┼───────┤ │
│  │ level_1  │ POSTED  │ ✓       │ 45,000.00  │ 0.001    │ 1      │ 0     │ │
│  │ level_2  │ POSTED  │ ✓       │ 45,500.00  │ 0.001    │ 1      │ 0     │ │
│  │ level_3  │ CREATED │ ✗       │ 44,800.00  │ 0.002    │ 0      │ 0     │ │
│  │ level_4  │ POSTED  │ ✓       │ 46,000.00  │ 0.001    │ 0      │ 0     │ │
│  │ level_5  │ FILLED  │ ✓       │ 44,500.00  │ 0.001    │ 0      │ 1     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Actions ────────────────────────────────────────────────────────┐ │
│  │                                                                     │ │
│  │  [V] View Level Details    - View full configuration of a level   │ │
│  │  [C] Create New Level      - Create a new level                    │ │
│  │  [E] Edit Level            - Edit existing level configuration     │ │
│  │  [T] Toggle Level          - Enable/disable a level                │ │
│  │  [D] Delete Level          - Delete a level (cancels orders)        │ │
│  │  [R] Refresh              - Refresh level list                    │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  Selected Level: [level_1]                                              │
│  [ESC] Back                                                              │
└─────────────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
- `V` - View level details
- `C` - Create new level
- `E` - Edit level
- `T` - Toggle level enabled/disabled
- `D` - Delete level
- `R` - Refresh
- `ESC` - Back to Trading Monitor

---

### 4. View Level Details

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Level Details: level_1                          [ESC] Back             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─ Basic Information ───────────────────────────────────────────────┐ │
│  │  ID: level_1                                                       │ │
│  │  Status: POSTED                                                    │ │
│  │  Enabled: ✓                                                        │ │
│  │  Created: 2024-01-15 10:30:00                                      │ │
│  │  Last Updated: 2024-01-15 14:20:00                                 │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Configuration ───────────────────────────────────────────────────┐ │
│  │  Symbol: BTC/USDT                                                  │ │
│  │  Side: BUY                                                         │ │
│  │  Order Type: LIMIT                                                 │ │
│  │  Price: 45,000.00 (or indicator: sma_20_btcusdt * 0.99)           │ │
│  │  Quantity: 0.001 (or indicator: balance_percent * 0.1)            │ │
│  │  Risk Limit: 100.00                                                │ │
│  │  Max Position Size: 0.01                                           │ │
│  │  Repost On Fill: ✓                                                │ │
│  │  Repost On Cancel: ✓                                              │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Statistics ───────────────────────────────────────────────────────┐ │
│  │  Total Orders: 1                                                   │ │
│  │  Filled Orders: 0                                                   │ │
│  │  Cancelled Orders: 0                                               │ │
│  │  Total Filled Quantity: 0.000                                      │ │
│  │  Last Fill Time: -                                                 │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Active Orders ────────────────────────────────────────────────────┐ │
│  │  Order ID    │ Side │ Price      │ Quantity │ Status               │ │
│  ├──────────────┼──────┼────────────┼──────────┼──────────────────────┤ │
│  │ #12345678    │ BUY  │ 45,000.00  │ 0.001    │ NEW                   │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  [E] Edit  [T] Toggle  [D] Delete  [ESC] Back                            │
└─────────────────────────────────────────────────────────────────────────┘
```

---

### 5. Create/Edit Level

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Create New Level                                    [ESC] Cancel        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─ Level Configuration ─────────────────────────────────────────────┐ │
│  │                                                                     │ │
│  │  Level ID:        [level_6________________________]                │ │
│  │  Symbol:          [BTC/USDT]  (Select from configured pairs)       │ │
│  │  Side:            [BUY ▼]     (BUY/SELL)                           │ │
│  │  Order Type:      [LIMIT ▼]   (LIMIT/MARKET)                       │ │
│  │                                                                     │ │
│  │  Price:            [45000.00]   (Decimal or indicator expression)   │ │
│  │                    Examples: "45000", "sma_20_btcusdt * 0.99"       │ │
│  │                                                                     │ │
│  │  Quantity:        [0.001]      (Decimal or indicator expression)    │ │
│  │                    Examples: "0.001", "balance_percent * 0.1"       │ │
│  │                                                                     │ │
│  │  Risk Limit:       [100.00]     (Max loss for this level)          │ │
│  │  Max Position:     [0.01]       (Max position size)                 │ │
│  │                                                                     │ │
│  │  Repost On Fill:   [✓]          (Repost when order fills)          │ │
│  │  Repost On Cancel: [✓]          (Repost when order cancels)         │ │
│  │  Enabled:          [✓]          (Start level immediately)            │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Validation ────────────────────────────────────────────────────────┐ │
│  │  ✓ Level ID is unique                                               │ │
│  │  ✓ Symbol is valid                                                  │ │
│  │  ✓ Price is valid                                                   │ │
│  │  ✓ Quantity is valid                                                │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  [S] Save  [ESC] Cancel                                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

**Note:** Edit Level screen is identical but pre-filled with existing values.

---

### 6. System Monitor

```
┌─────────────────────────────────────────────────────────────────────────┐
│ System Monitor                                    [ESC] Back to Menu    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─ Connectivity Status ──────────────────────────────────────────────┐ │
│  │  Exchange (Binance):     [✓ Connected]  Last Check: 14:23:45      │ │
│  │  WebSocket (User Data):  [✓ Connected]  Last Update: 14:23:44    │ │
│  │  WebSocket (Market):     [✓ Connected]  Last Update: 14:23:44    │ │
│  │  MongoDB:                [✓ Connected]  Last Check: 14:23:40    │ │
│  │  gRPC Server:            [✓ Listening]  Port: 50051              │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Channel Health ──────────────────────────────────────────────────┐ │
│  │  Channel              │ Status │ Lag Count │ Buffer Usage         │ │
│  ├───────────────────────┼────────┼───────────┼──────────────────────┤ │
│  │  Market Data          │ ✓ OK   │ 0         │ 45/1000 (4.5%)      │ │
│  │  Exchange Updates     │ ✓ OK   │ 0         │ 12/1000 (1.2%)      │ │
│  │  Indicator Updates    │ ✓ OK   │ 0         │ 8/1000 (0.8%)       │ │
│  │  Shutdown             │ ✓ OK   │ 0         │ 0/1 (0%)            │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Service Status ───────────────────────────────────────────────────┐ │
│  │  Service              │ Status │ Uptime    │ Last Activity        │ │
│  ├───────────────────────┼────────┼───────────┼──────────────────────┤ │
│  │  Trading Engine       │ ✓ Running │ 2h 15m │ 14:23:44            │ │
│  │  Market Data Engine   │ ✓ Running │ 2h 15m │ 14:23:44            │ │
│  │  Exchange Engine      │ ✓ Running │ 2h 15m │ 14:23:44            │ │
│  │  Indicator Service    │ ✓ Running │ 2h 15m │ 14:23:44            │ │
│  │  Asset Service        │ ✓ Running │ 2h 15m │ 14:23:00            │ │
│  │  Level Manager        │ ✓ Running │ 2h 15m │ 14:23:44            │ │
│  │  Risk Manager         │ ✓ Running │ 2h 15m │ 14:23:44            │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Performance Metrics ──────────────────────────────────────────────┐ │
│  │  System Uptime:        2 hours 15 minutes 32 seconds                 │ │
│  │  CPU Usage:            5.2%                                          │ │
│  │  Memory Usage:         125 MB / 512 MB (24.4%)                       │ │
│  │  Active Connections:   3 (2 WebSocket, 1 gRPC)                      │ │
│  │  Total Orders Placed:  15                                            │ │
│  │  Total Trades:         8                                             │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Protocol Health ──────────────────────────────────────────────────┐ │
│  │  Exchange Protocol:    [✓ Healthy]  Last Error: None                │ │
│  │  Market Data Protocol: [✓ Healthy]  Last Error: None                │ │
│  │  Reconciliation:       [✓ OK]      Last Run: 14:20:00 (3m ago)     │ │
│  │  Risk System:          [✓ OK]      Circuit Breaker: Closed          │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  [R] Refresh  [ESC] Back                                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
- `R` - Refresh all data
- `ESC` - Back to Main Menu

---

### 7. Live Config

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Live Config                                        [ESC] Back to Menu    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─ Current Configuration ───────────────────────────────────────────┐ │
│  │  Mode: [Levels]                                                      │ │
│  │  Config File: config/levels.json                                     │ │
│  │  Last Modified: 2024-01-15 10:30:00                                  │ │
│  │  Status: [✓ Loaded]                                                  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Paste New Configuration ─────────────────────────────────────────┐ │
│  │                                                                     │ │
│  │  {                                                                  │ │
│  │    "levels": [                                                      │ │
│  │      {                                                              │ │
│  │        "id": "level_1",                                            │ │
│  │        "symbol": { "base": "BTC", "quote": "USDT" },               │ │
│  │        "side": "Buy",                                              │ │
│  │        "order_type": "Limit",                                      │ │
│  │        "price": "45000.00",                                        │ │
│  │        "quantity": "0.001",                                         │ │
│  │        "enabled": true,                                            │ │
│  │        "risk_limit": "100.00",                                     │ │
│  │        "max_position_size": "0.01",                                │ │
│  │        "repost_on_fill": true,                                     │ │
│  │        "repost_on_cancel": true                                    │ │
│  │      },                                                             │ │
│  │      ...                                                            │ │
│  │    ],                                                               │ │
│  │    "risk_limits": {                                                │ │
│  │      "max_total_position": "10000.00",                              │ │
│  │      "max_total_orders": 50,                                       │ │
│  │      "daily_loss_limit_all_pairs": "500.00"                        │ │
│  │    }                                                                │ │
│  │  }                                                                  │ │
│  │                                                                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Validation Status ────────────────────────────────────────────────┐ │
│  │  ✓ JSON is valid                                                    │ │
│  │  ✓ Contains "levels" array                                         │ │
│  │  ✓ All level IDs are unique                                         │ │
│  │  ✓ All level configurations are valid                                │ │
│  │  ✓ Risk limits are valid                                            │ │
│  │  ⚠ 3 levels will be removed (have active orders - will be cancelled)│ │
│  │  ✓ 5 new levels will be added                                       │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Update Preview ───────────────────────────────────────────────────┐ │
│  │  Current Levels: 8                                                  │ │
│  │  New Levels: 5                                                       │ │
│  │  Levels to Remove: 3 (with active orders)                          │ │
│  │  Levels to Add: 5                                                   │ │
│  │  Risk Limits: Will be updated                                       │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Actions ──────────────────────────────────────────────────────────┐ │
│  │  [V] Validate JSON        - Validate JSON structure and content    │ │
│  │  [P] Preview Changes       - Show what will change                 │ │
│  │  [A] Apply Configuration   - Replace config and update state        │ │
│  │  [B] Backup Current        - Backup current config before applying  │ │
│  │  [R] Reset                 - Clear paste area                       │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  Status: [Ready]  Last Action: None                                      │
│  [ESC] Back                                                              │
└─────────────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
- `V` - Validate JSON
- `P` - Preview changes
- `A` - Apply configuration (with confirmation)
- `B` - Backup current config
- `R` - Reset/clear paste area
- `ESC` - Back to Main Menu

---

### 8. Trading Disabled Warning (When Toggling Off)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Trading Disabled                                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ⚠ Trading has been DISABLED                                            │
│                                                                          │
│  The following actions have been performed:                              │
│                                                                          │
│  ✓ All new order placements are blocked                                 │
│  ✓ All active orders have been cancelled                                │
│  ✓ Level execution is paused                                           │
│  ✓ Strategy execution is paused                                          │
│                                                                          │
│  Active orders cancelled: 3                                              │
│  - Order #12345678 (level_1)                                            │
│  - Order #12345679 (level_2)                                            │
│  - Order #12345680 (level_3)                                            │
│                                                                          │
│  You can re-enable trading by pressing [T] in the main menu.            │
│                                                                          │
│  [OK]                                                                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

### 9. Trading Enabled Confirmation (When Toggling On)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Trading Enabled                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ✓ Trading has been ENABLED                                             │
│                                                                          │
│  The following actions have been performed:                              │
│                                                                          │
│  ✓ New order placements are allowed                                     │
│  ✓ Level execution is active                                           │
│  ✓ Strategy execution is active                                         │
│                                                                          │
│  Active levels: 5                                                       │
│  Paused levels: 2                                                       │
│                                                                          │
│  Trading is now active. Orders will be placed according to your        │
│  level and strategy configurations.                                      │
│                                                                          │
│  [OK]                                                                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

### Design Notes

1. **Color Coding:**
   - Green (✓): Success/Active/Connected
   - Red (✗/❌): Error/Disabled/Disconnected
   - Yellow (⚠): Warning
   - Blue: Information
   - White: Normal text

2. **Status Indicators:**
   - `✓` = Active/Enabled/Connected/OK
   - `✗` = Inactive/Disabled/Disconnected
   - `⚠` = Warning
   - `❌` = Error

3. **Keyboard Navigation:**
   - All menus support `ESC` to go back
   - Number keys (1-9) for menu items
   - Letter keys for actions (context-dependent)
   - Tab for field navigation in forms

4. **Real-time Updates:**
   - Trading Monitor updates every 2 seconds (configurable)
   - System Monitor updates every 5 seconds
   - Status indicators update immediately on state changes

5. **Responsive Design:**
   - All screens adapt to terminal size
   - Minimum terminal size: 80x24
   - Recommended: 120x40 or larger

## Indicator Display in TUI

### Indicator Values Window

**Purpose**: Display real-time indicator values for all registered indicators.

**Layout**:
```
┌─────────────────────────────────────────────────────────┐
│ Indicator Values                    [BTCUSDT] [ETHUSDT] │
├─────────────────────────────────────────────────────────┤
│ ID              │ Name │ Pair    │ Value    │ Period   │
├─────────────────┼──────┼─────────┼──────────┼──────────┤
│ sma_20_btcusdt  │ SMA  │ BTCUSDT │ 45,230.5 │ 20       │
│ ema_12_btcusdt  │ EMA  │ BTCUSDT │ 45,180.2 │ 12       │
│ golden_cross_btc│ *GC  │ BTCUSDT │ 50.3     │ 2        │
│ rsi_14_btcusdt  │ RSI  │ BTCUSDT │ 65.4     │ 14       │
└─────────────────────────────────────────────────────────┘
```

**Features**:
- **Color Coding**:
  - Green: Bullish indicators (e.g., price above SMA)
  - Red: Bearish indicators (e.g., price below SMA)
  - Yellow: Neutral/warning values
- **Composite Indicator Marker**: `*` prefix for composite indicators
- **Sorting**: By pair, by name, by value
- **Filtering**: By pair, by indicator type (simple/composite)
- **Real-time Updates**: Values update as indicators recalculate

### Available Indicators Window

**Purpose**: Display all available indicators (simple and composite) with their configurations. Used for creating new composite indicators in TUI.

**Layout**:
```
┌─────────────────────────────────────────────────────────┐
│ Available Indicators                                     │
├─────────────────────────────────────────────────────────┤
│ Simple Indicators:                                       │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ ID              │ Name │ Pair    │ Period │ Source  │ │
│ ├─────────────────┼──────┼─────────┼────────┼─────────┤ │
│ │ sma_20_btcusdt  │ SMA  │ BTCUSDT │ 20     │ Close   │ │
│ │ ema_12_btcusdt  │ EMA  │ BTCUSDT │ 12     │ Close   │ │
│ │ rsi_14_btcusdt  │ RSI  │ BTCUSDT │ 14     │ Close   │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                           │
│ Composite Indicators:                                    │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ ID              │ Name        │ Pair    │ Expression│ │
│ ├─────────────────┼─────────────┼─────────┼───────────┤ │
│ │ golden_cross_btc│ Golden Cross│ BTCUSDT │ EMA50-EMA200│ │
│ │ macd_btcusdt    │ MACD        │ BTCUSDT │ EMA12-EMA26│ │
│ └─────────────────────────────────────────────────────┘ │
│                                                           │
│ [Create Composite] [Edit] [Delete] [View Expression]      │
└─────────────────────────────────────────────────────────┘
```

**Features**:
- **Simple Indicators Table**: Shows all simple indicators with their configuration
- **Composite Indicators Table**: Shows all composite indicators with their expression
- **Expression Display**: For composite indicators, show readable expression (e.g., "EMA50 - EMA200")
- **Actions**:
  - **Create Composite**: Open composite indicator builder
  - **Edit**: Edit existing composite indicator
  - **Delete**: Remove indicator
  - **View Expression**: Show full expression tree in JSON format

### Composite Indicator Builder (TUI)

**Purpose**: Create new composite indicators using available simple indicators.

**Layout**:
```
┌─────────────────────────────────────────────────────────┐
│ Create Composite Indicator                               │
├─────────────────────────────────────────────────────────┤
│ ID: [golden_cross_btcusdt________]                       │
│ Name: [Golden Cross________________]                     │
│ Pair: [BTCUSDT]                                          │
│                                                           │
│ Expression Builder:                                      │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ Available Indicators:                                 │ │
│ │ • sma_20_btcusdt (SMA, Period: 20)                    │ │
│ │ • ema_50_btcusdt (EMA, Period: 50)                    │ │
│ │ • ema_200_btcusdt (EMA, Period: 200)                   │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                           │
│ Expression:                                              │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ [ema_50_btcusdt] - [ema_200_btcusdt]               │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                           │
│ Operations: [+][-][*][/][min][max][abs][sqrt][if]        │
│                                                           │
│ Dependencies: ema_50_btcusdt, ema_200_btcusdt           │
│                                                           │
│ [Validate] [Save] [Cancel]                                │
└─────────────────────────────────────────────────────────┘
```

**Features**:
- **Expression Builder**: Visual builder for creating expressions
- **Available Indicators List**: Shows all simple indicators that can be used
- **Operations Panel**: Buttons for arithmetic, functions, and conditionals
- **Dependency Validation**: Automatically extracts and validates dependencies
- **Expression Preview**: Shows readable expression as user builds it
- **Validation**: Checks for circular dependencies and missing indicators

### Indicator Data in gRPC

**Data Flow**:
1. **Indicator Engine** → Calculates indicator values
2. **Indicator Engine** → Publishes to channel (`IndicatorUpdate`)
3. **Trading Engine** → Subscribes to indicator updates
4. **Trading Engine** → Aggregates indicator data
5. **gRPC Server** → Includes indicators in `MonitoringData`
6. **TUI** → Receives and displays indicator values

**gRPC Service Methods** (to be added):
```protobuf
service TradingCliService {
    // ... existing methods ...
    
    // Indicator management
    rpc GetIndicators(GetIndicatorsRequest) returns (GetIndicatorsResponse);
    rpc GetIndicatorMetadata(GetIndicatorMetadataRequest) returns (GetIndicatorMetadataResponse);
    rpc CreateCompositeIndicator(CreateCompositeIndicatorRequest) returns (CreateCompositeIndicatorResponse);
    rpc UpdateCompositeIndicator(UpdateCompositeIndicatorRequest) returns (UpdateCompositeIndicatorResponse);
    rpc DeleteIndicator(DeleteIndicatorRequest) returns (DeleteIndicatorResponse);
}
```

**Indicator Data Structure**:
- **Simple Indicators**: Include `id`, `name`, `pair`, `value`, `period`, `source`
- **Composite Indicators**: Include `id`, `name`, `pair`, `value`, `dependencies`, `expression_json`
- **Real-time Updates**: Indicator values are included in `MonitoringDataUpdate` stream

### Display Format Examples

**Simple Indicator**:
```
sma_20_btcusdt | SMA | BTCUSDT | 45,230.50 | 20 | Close
```

**Composite Indicator**:
```
golden_cross_btcusdt | *Golden Cross | BTCUSDT | 50.30 | 2 deps | EMA50-EMA200
```

**Expression Display**:
- **Arithmetic**: `EMA50 - EMA200`
- **Functions**: `min(SMA20, EMA12)`, `abs(RSI - 50)`
- **Conditionals**: `if(EMA50 > EMA200, 1, 0)`

## Live Config Window

**Purpose**: Runtime configuration updates without restart. Allows modifying level values, risk limits, and bot configuration while the trading system is running.

**Layout**:
```
┌─────────────────────────────────────────────────────────┐
│ Live Config Updates                    [Connected ✓]     │
├─────────────────────────────────────────────────────────┤
│                                                           │
│ Update Type: [●] Level  [ ] Risk Limit  [ ] Bot Config  │
│                                                           │
│ ┌─ Level Update ──────────────────────────────────────┐ │
│ │ Level ID: [level_1________________]                 │ │
│ │                                                       │ │
│ │ Field: [Enabled ▼]                                   │ │
│ │ Value: [true ▼]                                      │ │
│ │                                                       │ │
│ │ Available Fields:                                    │ │
│ │ • Price (Decimal)                                     │ │
│ │ • Quantity (Decimal)                                 │ │
│ │ • Enabled (Boolean)                                  │ │
│ │ • RiskLimit (Decimal)                                │ │
│ │ • MaxPositionSize (Decimal)                          │ │
│ │ • RepostOnFill (Boolean)                             │ │
│ │ • RepostOnCancel (Boolean)                           │ │
│ │                                                       │ │
│ │ [Validate] [Apply Update] [Cancel]                   │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                           │
│ ┌─ Update History ────────────────────────────────────┐ │
│ │ 2024-01-15 14:23:45 │ level_1 │ Enabled │ true  │ ✓ │ │
│ │ 2024-01-15 14:20:12 │ level_2 │ Price   │ 45230 │ ✓ │ │
│ │ 2024-01-15 14:18:33 │ risk   │ MaxPos  │ 10000 │ ✓ │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                           │
│ Status: [Ready]  Last Update: 2024-01-15 14:23:45        │
└─────────────────────────────────────────────────────────┘
```

**Features**:
- **Update Types**: Switch between Level, Risk Limit, and Bot Config updates
- **Level Updates**:
  - Select level by ID (dropdown or autocomplete)
  - Select field to update (dropdown with available fields)
  - Enter new value (type-specific input: decimal, boolean, string)
  - Field validation before applying
- **Risk Limit Updates**:
  - Select limit type (TotalMaxPosition, MaxTotalOrders, DailyLossLimitAllPairs)
  - Enter new value (decimal)
  - Validation against current positions/orders
- **Bot Config Updates**:
  - Enter config path (dot-separated, e.g., "reconciliation.exchange_level_interval_ms")
  - Enter new value (type-specific input)
  - Path validation
- **Update History**: Shows recent updates with timestamp, target, field, value, and status
- **Real-time Status**: Connection status, last update time, validation results
- **Validation Feedback**: Shows validation errors before applying
- **Audit Trail**: All updates logged to audit log file

**Connection to LiveConfig Service**:
- **Channel-based Communication**: LiveConfig service uses channel-based API internally (no HTTP dependencies)
- **gRPC Integration**: TUI connects via gRPC to the runner's gRPC server, which forwards requests to LiveConfig service
- **Client Integration**: The runner maintains a `LiveConfigClient` instance that communicates with the LiveConfig service via channels
- **Streaming Updates**: Receives real-time updates when configuration changes occur via the `StreamConfigUpdates` gRPC stream
- **Error Handling**: Displays validation errors and update failures
- **Connection Status**: Shows connection status to LiveConfig service

**Implementation Details**:

1. **Update Flow**:
   ```
   User Input (TUI) → gRPC Request → Runner gRPC Server → 
   LiveConfig Service (Channel API) → Validation → Update → 
   Persist → Response → gRPC Response → TUI Update
   ```

2. **Streaming Updates**:
   - Subscribe to `StreamConfigUpdates` gRPC stream from runner
   - Display updates in history panel
   - Update status indicators in real-time

3. **Error Handling**:
   - Display validation errors inline
   - Show connection errors
   - Handle timeout scenarios
   - Show persistence errors

**Navigation**:
- Access from main menu: `L` key or "Live Config" option
- Keyboard shortcuts:
  - `Tab`: Switch between update types
  - `Enter`: Apply update
  - `Esc`: Cancel/Back
  - `V`: Validate current input
  - `H`: Show update history
  - `R`: Refresh status

## Integration in Runner

### Runner Main (`runner/src/main.rs`)

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ... existing setup
    
    // Create channel pairs
    let (market_data_tx, market_data_rx) = market_data_channel();
    let (exchange_tx, exchange_rx) = exchange_channel();
    
    // Start services with channels
    // ...
    
    // Create gRPC server
    let grpc_server = TradingGrpcServer::new(
        level_manager.clone(),
        trading_state.clone(),
        risk_manager.clone(),
        aggregator.clone(),
    );
    
    // Start gRPC server
    let grpc_addr = "[::1]:50051".parse()?;
    tokio::spawn(async move {
        grpc_server.start(grpc_addr).await.unwrap();
    });
    
    // Wait for shutdown
    // ...
}
```

## Performance Considerations

### 1. gRPC Streaming
- **Binary protocol**: Efficient protobuf serialization
- **HTTP/2 multiplexing**: Lower latency
- **Streaming**: Real-time updates with backpressure
- **Batching**: Batch updates (e.g., every 2 seconds)

### 2. Data Aggregation
- **Cached aggregation**: Cache aggregated data with TTL
- **Incremental updates**: Only recalculate changed parts
- **Efficient serialization**: Use protobuf for gRPC

### 3. TUI Rendering
- **Incremental updates**: Only re-render changed sections
- **Frame rate limiting**: Cap at 60 FPS
- **Debouncing**: Debounce rapid updates

## Success Criteria

- [ ] gRPC server in runner exposes monitoring data
- [ ] TUI connects via gRPC and receives streaming updates
- [ ] Level management works via gRPC
- [ ] Real-time monitoring data displayed correctly
- [ ] System health monitoring functional
- [ ] Performance: < 2s update latency
- [ ] All TUI screens render correctly
- [ ] LiveConfig window connects to LiveConfig service via gRPC (runner forwards to channel-based API)
- [ ] LiveConfig window displays update history and real-time status
- [ ] LiveConfig updates are validated before applying
- [ ] LiveConfig updates are persisted to MongoDB (if enabled)
- [ ] LiveConfig streaming updates work via gRPC stream

## Implementation Phases

### Phase 1: Core Infrastructure and Basic TUI
**Goal:** Get the foundation working with basic navigation

**Tasks:**
1. gRPC server setup in `runner`
   - Create `runner/src/grpc/` module structure
   - Proto definitions (`trading_cli.proto`)
   - Code generation setup (tonic, prost)
   - Basic server lifecycle

2. TradingControl service
   - `TradingControl` struct with enable/disable flag
   - Integration with `LevelExecutor` and `StrategyExecutor`
   - **Cancel all orders when disabling**
   - gRPC endpoint: `SetTradingEnabled(bool)`

3. Basic TUI structure (`cli/` crate)
   - Create new `cli/` crate
   - Setup `ratatui` framework
   - Basic app structure and navigation
   - Main Menu screen (static, no data yet)

4. gRPC client in TUI
   - Client connection setup
   - Basic error handling
   - Connection status display

**Deliverables:**
- Main Menu renders and navigates
- Trading enable/disable works (blocks orders, cancels active)
- gRPC connection established
- Basic TUI framework ready

---

### Phase 2: Trading Monitor and Data Aggregation
**Goal:** Display real-time trading data

**Tasks:**
1. Monitoring data aggregation (`trading_engine/src/monitoring/`)
   - `Aggregator` for statistics (overall, per-pair, per-level)
   - `DataBuilder` for complete monitoring data
   - Subscribe to channels (exchange, market data, indicators)

2. gRPC streaming for monitoring
   - `StreamMonitoringData` RPC implementation
   - Periodic updates (every 2 seconds)
   - Efficient data serialization

3. Trading Monitor screen
   - Orderbook display
   - Indicators table
   - Open orders table
   - Balances display
   - Recent trades
   - Levels list (read-only initially)

4. Real-time updates
   - Auto-refresh every 2 seconds
   - Handle connection errors gracefully

**Deliverables:**
- Trading Monitor displays all data
- Real-time updates working
- Data flows from channels → aggregation → gRPC → TUI

---

### Phase 3: Level Management (Full CRUD)
**Goal:** Complete level management from TUI

**Tasks:**
1. Level management gRPC endpoints
   - `GetLevels`, `GetLevel` (view details)
   - `CreateLevel`, `UpdateLevel`, `DeleteLevel`
   - `ToggleLevel` (enable/disable)

2. Level Management screen
   - List all levels with status
   - View level details
   - Create new level form
   - Edit level form
   - Delete confirmation

3. Integration with LevelManager
   - All operations go through `LevelManager`
   - Persistence to MongoDB (if enabled)
   - Validation and error handling

4. Navigation flow
   - Access from Trading Monitor (press `L`)
   - Return to Trading Monitor

**Deliverables:**
- Full CRUD operations work
- Level Management screen complete
- Changes persist correctly

---

### Phase 4: System Monitor
**Goal:** Display system health and connectivity

**Tasks:**
1. Health tracking infrastructure
   - Channel health monitoring (lag detection)
   - Service status tracking
   - Connectivity checks (exchange, MongoDB)
   - Performance metrics collection

2. System health gRPC endpoints
   - `GetSystemHealth`
   - `GetProtocolHealth`
   - Health data aggregation

3. System Monitor screen
   - Connectivity status
   - Channel health table
   - Service status table
   - Performance metrics
   - Protocol health

4. Real-time updates
   - Auto-refresh every 5 seconds
   - Status indicators update immediately

**Deliverables:**
- System Monitor displays all health data
- Real-time status updates
- Channel lag detection working

---

### Phase 5: Live Config
**Goal:** Replace entire config without restart

**Tasks:**
1. Full config replacement in LiveConfigService
   - `ReplaceFullConfig` method
   - Parse and validate JSON
   - Cancel orders for removed levels
   - Update LevelManager/StrategyManager
   - Backup old config
   - Persist to MongoDB (if enabled)
   - Rollback mechanism

2. Live Config gRPC endpoints
   - `ReplaceConfig` RPC
   - Validation endpoint
   - Preview changes endpoint

3. Live Config screen
   - Config file display (current mode)
   - Paste area for new config
   - Validation status
   - Preview changes
   - Apply confirmation dialog

4. Error handling and recovery
   - Validation errors displayed
   - Rollback on failure
   - Backup/restore functionality

**Deliverables:**
- Full config replacement works
- Validation and preview working
- Safe rollback on errors

---

### Phase 6: Polish and Enhancements
**Goal:** Improve UX and add finishing touches

**Tasks:**
1. Color coding and styling
   - Green/red/yellow for status
   - Color-coded indicators
   - Better visual hierarchy

2. Error handling improvements
   - User-friendly error messages
   - Error overlays/dialogs
   - Connection retry logic

3. Performance optimizations
   - Efficient rendering (only update changed sections)
   - Frame rate limiting
   - Debouncing rapid updates

4. Additional features
   - Keyboard shortcuts help
   - Loading indicators
   - Success/warning dialogs
   - Terminal resize handling

5. Testing and bug fixes
   - Test all flows
   - Handle edge cases
   - Performance testing

**Deliverables:**
- Polished UI with colors
- Robust error handling
- Smooth performance
- Production-ready TUI

---

## Notes

- **TUI IS the monitoring interface** - no separate monitoring service
- **gRPC server in runner** - exposes data to external TUI process
- **Internal communication via channels** - ultra-low latency
- **External communication via gRPC** - required for external process
- **Data aggregation in Trading Engine** - prepares monitoring data
- **Streaming for real-time updates** - efficient binary protocol
- **Trading Control** - When disabled, all orders are cancelled immediately

