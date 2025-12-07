# Proposal 07: Terminal User Interface (TUI) CLI

## Overview

This proposal defines a production-ready Terminal User Interface (TUI) CLI for the trading system. The TUI provides an intuitive, menu-driven interface for level configuration, real-time trading monitoring, and system health monitoring. It serves as the primary interface for traders to interact with the system, configure levels, monitor positions, and manage risk.

### Goals

1. **Production-Ready Interface**: A polished, professional TUI that rivals web-based interfaces
2. **Real-Time Monitoring**: Live updates for orderbooks, balances, orders, trades, and risk metrics
3. **Level Management**: Complete CRUD operations for trading levels with runtime configuration
4. **Visual Clarity**: Color-coded displays, markers, and intuitive navigation
5. **Performance**: Efficient rendering, minimal latency, smooth updates
6. **User Experience**: Intuitive navigation, keyboard shortcuts, multi-view support

### Prerequisites

This proposal assumes the following proposals are implemented:
- **Proposal 01**: Library Separation Refactoring
- **Proposal 02**: Level-Based Trading System
- **Proposal 03**: Monitoring System
- **Proposal 04**: Data Handling and Persistence
- **Proposal 05**: Risk System
- **Proposal 06**: Exchange Market Data Integration

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    TUI CLI Application                   │
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
│              Trading System (gRPC Server)                 │
│  - Level Manager                                          │
│  - Trading State                                          │
│  - Risk Manager                                           │
│  - Monitoring Service                                     │
└───────────────────────────────────────────────────────────┘
```

### Component Structure

```
trading_system/
├── src/
│   └── cli/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs                 # Entry point
│       │   ├── app.rs                  # Main application state
│       │   ├── client.rs               # gRPC client
│       │   ├── screens/
│       │   │   ├── mod.rs
│       │   │   ├── main_menu.rs        # Main menu screen
│       │   │   ├── level_setup/
│       │   │   │   ├── mod.rs
│       │   │   │   ├── menu.rs         # Level setup menu
│       │   │   │   ├── list.rs         # Level list view
│       │   │   │   ├── create.rs       # Create level form
│       │   │   │   ├── edit.rs         # Edit level form
│       │   │   │   └── delete.rs        # Delete confirmation
│       │   │   ├── trading_monitor/
│       │   │   │   ├── mod.rs
│       │   │   │   ├── menu.rs         # Trading monitor menu
│       │   │   │   ├── dashboard.rs     # Overview dashboard
│       │   │   │   ├── orderbook.rs    # Orderbook view
│       │   │   │   ├── balances.rs     # Balances view
│       │   │   │   ├── orders.rs        # Open orders view
│       │   │   │   ├── trades.rs        # Trades view
│       │   │   │   ├── positions.rs    # Positions view
│       │   │   │   ├── risk.rs          # Risk metrics view
│       │   │   │   └── multi_view.rs    # Split-screen view
│       │   │   └── system_monitor/
│       │   │       ├── mod.rs
│       │   │       ├── menu.rs         # System monitor menu
│       │   │       ├── health.rs        # System health
│       │   │       ├── protocol.rs      # Protocol health
│       │   │       ├── connections.rs   # Connection status
│       │   │       ├── performance.rs   # Performance metrics
│       │   │       └── logs.rs          # Log viewer
│       │   ├── widgets/
│       │   │   ├── mod.rs
│       │   │   ├── orderbook.rs         # Orderbook widget
│       │   │   ├── table.rs             # Generic table widget
│       │   │   ├── form.rs              # Form widget
│       │   │   ├── status_bar.rs        # Status bar widget
│       │   │   ├── legend.rs            # Legend widget
│       │   │   └── metrics_box.rs       # Metrics box widget
│       │   ├── formatters/
│       │   │   ├── mod.rs
│       │   │   ├── decimal.rs           # Decimal formatting
│       │   │   ├── price.rs             # Price formatting
│       │   │   ├── quantity.rs          # Quantity formatting
│       │   │   ├── time.rs               # Time formatting
│       │   │   └── pnl.rs               # P&L formatting
│       │   ├── styles/
│       │   │   ├── mod.rs
│       │   │   ├── colors.rs            # Color schemes
│       │   │   ├── themes.rs             # Theme definitions
│       │   │   └── markers.rs           # Visual markers
│       │   ├── navigation.rs            # Navigation state
│       │   ├── events.rs                # Event handling
│       │   └── config.rs                # CLI configuration
│       └── proto/                       # gRPC proto files (if needed)
│           └── trading_cli.proto
```

## User Experience Flow

### Main Menu

```
┌─────────────────────────────────────────────────────────┐
│  Trading System CLI                    [Q] Quit         │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │                                                    │   │
│  │  [1] Level Setup & Configuration                 │   │
│  │                                                    │   │
│  │  [2] Trading Monitoring                          │   │
│  │                                                    │   │
│  │  [3] System Monitoring                            │   │
│  │                                                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Status: ✓ Connected | Exchange: Binance | Pair: BTCUSDT│
│  Last Update: 10:25:00                                  │
└─────────────────────────────────────────────────────────┘
│  [↑↓] Navigate | [Enter] Select | [Q] Quit              │
└─────────────────────────────────────────────────────────┘
```

**Navigation**: Arrow keys or numbers (1-3), Enter to select, Q to quit.

### Level Setup Menu

```
┌─────────────────────────────────────────────────────────┐
│  Level Setup & Configuration      [← Back] [Q] Quit     │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │                                                    │   │
│  │  [1] View All Levels                              │   │
│  │                                                    │   │
│  │  [2] Create New Level                             │   │
│  │                                                    │   │
│  │  [3] Edit Level                                    │   │
│  │                                                    │   │
│  │  [4] Enable/Disable Level                         │   │
│  │                                                    │   │
│  │  [5] Delete Level                                  │   │
│  │                                                    │   │
│  │  [6] Import from Config File                      │   │
│  │                                                    │   │
│  │  [7] Export to Config File                       │   │
│  │                                                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Active Levels: 4 | Disabled: 1 | Total: 5              │
└─────────────────────────────────────────────────────────┘
```

### View All Levels

```
┌─────────────────────────────────────────────────────────┐
│  All Levels              [← Back] [E] Edit [D] Delete   │
├─────────────────────────────────────────────────────────┤
│ ID         │ Pair    │ Side │ Price   │ Qty   │ Status │
├────────────┼─────────┼──────┼─────────┼───────┼────────┤
│ ▶ buy_1    │ BTCUSDT │ BUY  │ 45000.00│ 0.002 │ ✓ Active│
│   buy_2    │ BTCUSDT │ BUY  │ 44800.00│ 0.002 │ ✓ Active│
│   sell_1   │ BTCUSDT │ SELL │ 46000.00│ 0.002 │ ✗ Disabled│
│   sell_2   │ BTCUSDT │ SELL │ 46200.00│ 0.002 │ ✓ Active│
│   buy_eth  │ ETHUSDT │ BUY  │ 2500.00 │ 0.1   │ ✓ Active│
└─────────────────────────────────────────────────────────┘
│  [↑↓] Navigate | [Enter] View Details | [E] Edit | [D] Delete│
└─────────────────────────────────────────────────────────┘
```

**Features**:
- Color coding: Green for enabled, Red for disabled
- Selection indicator: `▶` shows current selection
- Status icons: `✓` for active, `✗` for disabled
- Keyboard shortcuts: E to edit, D to delete, Enter for details

### Create/Edit Level Form

```
┌─────────────────────────────────────────────────────────┐
│  Create Level                [← Back] [S] Save [Q] Quit│
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Level ID:     [buy_btc_1________________]             │
│  Pair:         [BTCUSDT________]                       │
│  Side:         [BUY ▼]                                 │
│  Price:        [45000.00______] (or use indicator)      │
│  Quantity:     [0.002________]                         │
│  Order Type:   [LIMIT ▼]                                │
│  Indicator:    [SMA_20 ▼] (optional)                   │
│  Enabled:      [✓]                                      │
│                                                          │
│  Validation: ✓ All fields valid                         │
│                                                          │
└─────────────────────────────────────────────────────────┘
│  [Tab] Next Field | [Shift+Tab] Previous | [S] Save     │
└─────────────────────────────────────────────────────────┘
```

**Features**:
- Field-by-field navigation with Tab
- Dropdown menus for enums (Side, Order Type, Indicator)
- Real-time validation
- Visual feedback for valid/invalid fields

### Trading Monitoring Menu

```
┌─────────────────────────────────────────────────────────┐
│  Trading Monitoring              [← Back] [Q] Quit       │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │                                                    │   │
│  │  [1] Dashboard (Overview)                        │   │
│  │                                                    │   │
│  │  [2] Orderbooks                                  │   │
│  │                                                    │   │
│  │  [3] Balances                                    │   │
│  │                                                    │   │
│  │  [4] Open Orders                                 │   │
│  │                                                    │   │
│  │  [5] Trades                                      │   │
│  │                                                    │   │
│  │  [6] Positions                                   │   │
│  │                                                    │   │
│  │  [7] Risk Metrics                                │   │
│  │                                                    │   │
│  │  [8] Multi-View (Split Screen)                   │   │
│  │                                                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Auto-refresh: ON (2s) | Last Update: 10:25:00          │
└─────────────────────────────────────────────────────────┘
```

### Dashboard (Overview)

```
┌─────────────────────────────────────────────────────────┐
│  Trading Dashboard            [← Back] [R] Refresh [Q]  │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Summary:                                                │
│  ┌─────────────────┐  ┌─────────────────┐              │
│  │ Total Balance   │  │ Active Orders   │              │
│  │ $10,450.00      │  │ 8               │              │
│  └─────────────────┘  └─────────────────┘              │
│  ┌─────────────────┐  ┌─────────────────┐              │
│  │ P&L (24h)       │  │ Open Positions  │              │
│  │ +$125.50        │  │ 2               │              │
│  └─────────────────┘  └─────────────────┘              │
│                                                          │
│  Recent Trades:                                         │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Time     │ Pair     │ Side │ Price  │ Qty │ P&L  │   │
│  ├──────────────────────────────────────────────────┤   │
│  │ 10:23:15 │ BTCUSDT  │ BUY  │ 45000  │0.002│ +5.2 │   │
│  │ 10:20:30 │ BTCUSDT  │ SELL │ 45100  │0.002│ +2.0 │   │
│  │ 10:18:45 │ BTCUSDT  │ BUY  │ 44950  │0.002│ -1.5 │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Risk Status: ✓ OK | Position: 20% | Daily P&L: +$125.50│
│  Last Update: 10:25:00 | Auto-refresh: ON (2s)          │
└─────────────────────────────────────────────────────────┘
```

### Orderbook View (Enhanced with Color Coding)

```
┌─────────────────────────────────────────────────────────┐
│  Orderbook: BTCUSDT            [← Back] [R] Refresh [Q] │
├─────────────────────────────────────────────────────────┤
│  Bids (Buy)                    │  Asks (Sell)            │
│  ┌──────────┬──────────┬──────┐│  ┌──────────┬──────────┐│
│  │ Price    │ Quantity │      ││  │ Price    │ Quantity ││
│  ├──────────┼──────────┼──────┤│  ├──────────┼──────────┤│
│  │ 45000.00 │ 0.5      │      ││  │ 45001.00 │ 0.3      ││
│  │ 44999.00 │ 0.8      │      ││  │ 45002.00 │ 0.6      ││
│  │ 44998.00 │ 1.2      │ ▶   ││  │ 45003.00 │ 0.4      ││
│  │          │ (0.2 ours)│     ││  │          │          ││
│  │ 44997.00 │ 0.9      │ ▶   ││  │ 45004.00 │ 0.7      │◀│
│  │          │ (0.9 ours)│     ││  │          │(0.7 ours)││
│  │ 44996.00 │ 1.5      │      ││  │ 45005.00 │ 0.5      ││
│  └──────────┴──────────┴──────┘│  └──────────┴──────────┘│
│                                                          │
│  Mid Price: 45000.50 | Spread: 1.00 | Last: 10:25:00    │
│  Your Orders: 2 bids (0.2 + 0.9), 1 ask (0.7)           │
│  Legend: [Green = Your Bids] [Red = Your Asks]         │
└─────────────────────────────────────────────────────────┘
│  [↑↓] Navigate | [Enter] View Details | [Q] Back        │
└─────────────────────────────────────────────────────────┘
```

**Visual Enhancements**:
- **Green text**: Your buy orders (bids)
- **Red text**: Your sell orders (asks)
- **Normal text**: Market orders
- **Markers**: `▶` for your bids, `◀` for your asks
- **Quantity breakdown**: Shows your quantity at each price level
- **Bold text**: Your orders stand out

### Balances View

```
┌─────────────────────────────────────────────────────────┐
│  Balances                      [← Back] [R] Refresh [Q] │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Symbol │ Available │ Locked │ Total   │ Value   │   │
│  ├──────────────────────────────────────────────────┤   │
│  │ BTC    │ 0.5       │ 0.1    │ 0.6     │ $27,000 │   │
│  │ USDT   │ 10,000    │ 200    │ 10,200  │ $10,200 │   │
│  │ ETH    │ 2.5       │ 0.0    │ 2.5     │ $6,250  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Total Value: $43,450.00                                 │
│  Last Update: 10:25:00 | Auto-refresh: ON (2s)          │
└─────────────────────────────────────────────────────────┘
```

### Open Orders View

```
┌─────────────────────────────────────────────────────────┐
│  Open Orders                   [← Back] [R] Refresh [Q] │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Order ID │ Pair    │ Side │ Price  │ Qty │ Status│   │
│  ├──────────────────────────────────────────────────┤   │
│  │ buy_1    │ BTCUSDT │ BUY  │ 45000  │0.002│ New   │   │
│  │ buy_2    │ BTCUSDT │ BUY  │ 44800  │0.002│ New   │   │
│  │ sell_1   │ BTCUSDT │ SELL │ 46000  │0.002│ New   │   │
│  │ sell_2   │ BTCUSDT │ SELL │ 46200  │0.002│ New   │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Total: 4 orders | [Enter] View Details | [C] Cancel    │
│  Last Update: 10:25:00 | Auto-refresh: ON (2s)          │
└─────────────────────────────────────────────────────────┘
```

### Risk Metrics View

```
┌─────────────────────────────────────────────────────────┐
│  Risk Metrics                    [← Back] [R] Refresh   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Position Limits:                                       │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Total Position: 0.1 BTC (Limit: 0.5 BTC)       │   │
│  │ Status: ✓ OK (20% of limit)                      │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Loss Limits:                                            │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Daily P&L: +$125.50 (Limit: -$1000.00)          │   │
│  │ Status: ✓ OK                                      │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Exposure:                                               │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Total Exposure: $4,500 (Limit: $10,000)        │   │
│  │ Status: ✓ OK (45% of limit)                      │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Circuit Breaker: ✓ Active                              │
│  Last Update: 10:25:00 | Auto-refresh: ON (2s)          │
└─────────────────────────────────────────────────────────┘
```

### Multi-View (Split Screen)

```
┌──────────────────────────┬──────────────────────────────┐
│  Balances                │  Open Orders                │
├──────────────────────────┼──────────────────────────────┤
│ Symbol │ Available │ Lock│ Order ID │ Price │ Qty │Stat│
│ BTC    │ 0.5       │ 0.1 │ buy_1    │ 45000 │0.002│New │
│ USDT   │ 10000     │ 200 │ buy_2    │ 44800 │0.002│New │
│ ETH    │ 2.5       │ 0.0 │ sell_1   │ 46000 │0.002│New │
│        │           │     │          │       │     │    │
│        │           │     │          │       │     │    │
└──────────────────────────┴──────────────────────────────┘
│  [Tab] Switch Focus | [←→] Resize | [Q] Close Split    │
└─────────────────────────────────────────────────────────┘
```

**Features**:
- Two-pane split view
- Tab to switch focus between panes
- Arrow keys to resize panes
- Configurable views (any combination)
- Independent refresh for each pane

### System Monitoring Menu

```
┌─────────────────────────────────────────────────────────┐
│  System Monitoring              [← Back] [Q] Quit        │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │                                                    │   │
│  │  [1] System Health                              │   │
│  │                                                    │   │
│  │  [2] Protocol Health                             │   │
│  │                                                    │   │
│  │  [3] Connection Status                           │   │
│  │                                                    │   │
│  │  [4] Performance Metrics                         │   │
│  │                                                    │   │
│  │  [5] Logs                                        │   │
│  │                                                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Uptime: 2d 5h 30m | Status: ✓ All Systems Operational│
└─────────────────────────────────────────────────────────┘
```

### System Health View

```
┌─────────────────────────────────────────────────────────┐
│  System Health                   [← Back] [R] Refresh   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Component Status:                                       │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Trading Engine:    ✓ Running                     │   │
│  │ Market Data:       ✓ Connected                   │   │
│  │ Exchange Engine:   ✓ Connected                   │   │
│  │ Level Manager:     ✓ Active                      │   │
│  │ Risk System:       ✓ Active                      │   │
│  │ Monitoring:        ✓ Active                      │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Uptime: 2d 5h 30m                                      │
│  Last Error: None                                        │
│  Last Update: 10:25:00 | Auto-refresh: ON (2s)          │
└─────────────────────────────────────────────────────────┘
```

### Protocol Health View

```
┌─────────────────────────────────────────────────────────┐
│  Protocol Health                 [← Back] [R] Refresh   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Exchange: Binance                                      │
│  ┌──────────────────────────────────────────────────┐   │
│  │ REST API:         ✓ OK (latency: 45ms)          │   │
│  │ WebSocket:        ✓ Connected                    │   │
│  │ Rate Limits:      ✓ OK (45% used)               │   │
│  │ Circuit Breaker:  ✓ Closed (healthy)            │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Internal Services:                                      │
│  ┌──────────────────────────────────────────────────┐   │
│  │ gRPC:             ✓ OK (latency: 2ms)           │   │
│  │ Redis:            ✓ Connected                    │   │
│  │ MongoDB:          ✓ Connected                    │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  Last Update: 10:25:00 | Auto-refresh: ON (2s)          │
└─────────────────────────────────────────────────────────┘
```

## Detailed Design

### 1. Application State Management

```rust
pub struct App {
    // Navigation
    pub navigation: NavigationState,
    
    // Current screen
    pub current_screen: Screen,
    
    // gRPC client
    pub client: TradingCliClient,
    
    // Local state cache
    pub state_cache: StateCache,
    
    // UI state
    pub ui_state: UiState,
    
    // Configuration
    pub config: CliConfig,
}

pub struct NavigationState {
    pub history: Vec<Screen>,
    pub current_index: usize,
}

pub struct StateCache {
    pub levels: Vec<Level>,
    pub balances: Vec<Balance>,
    pub orders: Vec<Order>,
    pub trades: Vec<Trade>,
    pub positions: Vec<Position>,
    pub risk_metrics: RiskMetrics,
    pub system_health: SystemHealth,
    pub last_update: Instant,
}

pub struct UiState {
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub input_buffer: String,
    pub focused_field: Option<String>,
    pub refresh_interval: Duration,
    pub auto_refresh: bool,
}
```

### 2. gRPC Client Integration

```rust
pub struct TradingCliClient {
    grpc_client: TradingSystemClient<Channel>,
    connection_string: String,
    reconnect_interval: Duration,
}

impl TradingCliClient {
    /// Connect to trading system
    pub async fn connect(&mut self) -> Result<(), CliError>;
    
    /// Get all levels
    pub async fn get_levels(&self) -> Result<Vec<Level>, CliError>;
    
    /// Create level
    pub async fn create_level(&self, config: LevelConfig) -> Result<String, CliError>;
    
    /// Update level
    pub async fn update_level(&self, level_id: &str, updates: LevelUpdates) -> Result<(), CliError>;
    
    /// Delete level
    pub async fn delete_level(&self, level_id: &str) -> Result<(), CliError>;
    
    /// Enable/disable level
    pub async fn toggle_level(&self, level_id: &str, enabled: bool) -> Result<(), CliError>;
    
    /// Get monitoring data stream
    pub async fn stream_monitoring_data(&self) -> Result<Receiver<MonitoringData>, CliError>;
    
    /// Get system health
    pub async fn get_system_health(&self) -> Result<SystemHealth, CliError>;
    
    /// Get protocol health
    pub async fn get_protocol_health(&self) -> Result<ProtocolHealth, CliError>;
}
```

### 3. Orderbook Widget with Color Coding

```rust
pub struct OrderbookWidget {
    orderbook: Orderbook,
    our_orders: Vec<Order>,
    color_scheme: ColorScheme,
    show_markers: bool,
    show_quantity_breakdown: bool,
}

impl OrderbookWidget {
    fn render_bids(&self, area: Rect, buf: &mut Buffer) {
        for (i, bid) in self.orderbook.bids.iter().enumerate() {
            let is_ours = self.is_our_order(bid.price, Side::Buy);
            let our_quantity = self.get_our_quantity_at_price(bid.price, Side::Buy);
            
            let style = if is_ours {
                Style::default()
                    .fg(self.color_scheme.our_bid_color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(self.color_scheme.market_order_color)
            };
            
            let marker = if is_ours && self.show_markers {
                "▶ "
            } else {
                "  "
            };
            
            let quantity_text = if is_ours && self.show_quantity_breakdown {
                format!("{} ({:?} ours)", bid.quantity, our_quantity)
            } else {
                format!("{}", bid.quantity)
            };
            
            // Render row with style
            // ...
        }
    }
    
    fn is_our_order(&self, price: Decimal, side: Side) -> bool {
        self.our_orders.iter().any(|order| {
            order.price == price && order.side == side
        })
    }
    
    fn get_our_quantity_at_price(&self, price: Decimal, side: Side) -> Decimal {
        self.our_orders
            .iter()
            .filter(|o| o.price == price && o.side == side)
            .map(|o| o.quantity)
            .sum()
    }
}
```

### 4. Color Scheme Configuration

```rust
pub struct ColorScheme {
    pub our_bid_color: Color,          // Green
    pub our_ask_color: Color,           // Red
    pub market_order_color: Color,      // White/Gray
    pub background_color: Color,
    pub selected_color: Color,
    pub enabled_color: Color,
    pub disabled_color: Color,
    pub error_color: Color,
    pub warning_color: Color,
    pub success_color: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            our_bid_color: Color::Green,
            our_ask_color: Color::Red,
            market_order_color: Color::White,
            background_color: Color::Black,
            selected_color: Color::Cyan,
            enabled_color: Color::Green,
            disabled_color: Color::Red,
            error_color: Color::Red,
            warning_color: Color::Yellow,
            success_color: Color::Green,
        }
    }
}

pub struct Theme {
    pub color_scheme: ColorScheme,
    pub markers: MarkerStyle,
    pub refresh_indicator: RefreshIndicatorStyle,
}

pub enum MarkerStyle {
    Arrows,      // ▶ ◀
    Stars,       // ★
    Dots,        // ●
    Custom(String, String),
}
```

### 5. Event Handling

```rust
pub enum AppEvent {
    Tick,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    MonitoringData(MonitoringData),
    SystemHealth(SystemHealth),
    Error(CliError),
}

pub async fn handle_events(
    app: &mut App,
    event: AppEvent,
) -> Result<(), CliError> {
    match event {
        AppEvent::Key(key) => {
            match key.code {
                KeyCode::Char('q') => app.should_quit = true,
                KeyCode::Char('Q') => app.should_quit = true,
                KeyCode::Esc => app.navigate_back(),
                KeyCode::Up => app.move_selection_up(),
                KeyCode::Down => app.move_selection_down(),
                KeyCode::Enter => app.select_current(),
                KeyCode::Tab => app.next_field(),
                KeyCode::BackTab => app.previous_field(),
                KeyCode::Char('r') | KeyCode::Char('R') => app.refresh(),
                KeyCode::Char('p') | KeyCode::Char('P') => app.toggle_auto_refresh(),
                _ => {}
            }
        }
        AppEvent::MonitoringData(data) => {
            app.update_cache(data);
        }
        AppEvent::Tick => {
            if app.ui_state.auto_refresh {
                app.refresh_if_needed().await?;
            }
        }
        _ => {}
    }
    Ok(())
}
```

### 6. Real-Time Updates

```rust
pub struct UpdateManager {
    refresh_interval: Duration,
    last_refresh: Instant,
    auto_refresh: bool,
    stream_receiver: Option<Receiver<MonitoringData>>,
}

impl UpdateManager {
    pub async fn start_stream(&mut self, client: &TradingCliClient) -> Result<(), CliError> {
        let receiver = client.stream_monitoring_data().await?;
        self.stream_receiver = Some(receiver);
        Ok(())
    }
    
    pub async fn handle_updates(&mut self, app: &mut App) -> Result<(), CliError> {
        if let Some(receiver) = &mut self.stream_receiver {
            while let Ok(data) = receiver.try_recv() {
                app.update_cache(data);
            }
        }
        
        if self.auto_refresh && self.last_refresh.elapsed() >= self.refresh_interval {
            app.refresh().await?;
            self.last_refresh = Instant::now();
        }
        
        Ok(())
    }
}
```

### 7. Form Handling

```rust
pub struct FormField {
    pub name: String,
    pub label: String,
    pub value: String,
    pub field_type: FieldType,
    pub validation: Option<Box<dyn Fn(&str) -> Result<(), String>>>,
    pub is_valid: bool,
    pub error_message: Option<String>,
}

pub enum FieldType {
    Text,
    Decimal,
    Integer,
    Dropdown(Vec<String>),
    Checkbox(bool),
}

pub struct LevelForm {
    pub fields: Vec<FormField>,
    pub current_field_index: usize,
    pub is_valid: bool,
}

impl LevelForm {
    pub fn validate(&mut self) {
        for field in &mut self.fields {
            if let Some(validator) = &field.validation {
                match validator(&field.value) {
                    Ok(()) => {
                        field.is_valid = true;
                        field.error_message = None;
                    }
                    Err(msg) => {
                        field.is_valid = false;
                        field.error_message = Some(msg);
                    }
                }
            }
        }
        self.is_valid = self.fields.iter().all(|f| f.is_valid);
    }
    
    pub fn to_level_config(&self) -> Result<LevelConfig, CliError> {
        // Convert form fields to LevelConfig
        // ...
    }
}
```

## Integration Points

### 1. gRPC Service Definition

The TUI CLI connects to the trading system via gRPC. The service should expose:

```protobuf
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
}
```

### 2. Data Models

The TUI uses the same data models as the trading system:

- `Level`, `LevelConfig` (from Proposal 02)
- `Order`, `Balance`, `Position`, `Trade` (from core_logic)
- `RiskMetrics` (from Proposal 05)
- `SystemHealth`, `ProtocolHealth` (new, for system monitoring)

### 3. Configuration

```rust
pub struct CliConfig {
    pub grpc_endpoint: String,
    pub refresh_interval_ms: u64,
    pub auto_refresh: bool,
    pub color_scheme: ColorScheme,
    pub theme: Theme,
    pub show_markers: bool,
    pub show_quantity_breakdown: bool,
}
```

## Performance Considerations

### 1. Efficient Rendering

- **Incremental Updates**: Only re-render changed sections
- **Frame Rate Limiting**: Cap at 60 FPS to reduce CPU usage
- **Lazy Loading**: Load data only when screen is visible
- **Debouncing**: Debounce rapid updates (e.g., orderbook changes)

### 2. Network Efficiency

- **gRPC Streaming**: Use streaming for real-time updates
- **Batching**: Batch multiple updates together
- **Connection Pooling**: Reuse gRPC connections
- **Backpressure**: Handle backpressure from streams

### 3. Memory Management

- **State Caching**: Cache only necessary data
- **TTL-based Expiration**: Expire stale cache entries
- **Bounded Buffers**: Limit buffer sizes for streams
- **Garbage Collection**: Clear unused state regularly

### 4. Update Frequency

- **Configurable Intervals**: Allow users to set refresh rates
- **Adaptive Updates**: Reduce frequency when idle
- **Manual Refresh**: Always allow manual refresh
- **Pause Updates**: Allow pausing updates when reading

## Keyboard Shortcuts

### Global Shortcuts

- `Q` / `Ctrl+C`: Quit application
- `Esc` / `Backspace`: Go back / Cancel
- `R`: Refresh current view
- `P`: Toggle auto-refresh
- `?`: Show help

### Navigation

- `↑` / `↓`: Move selection up/down
- `Enter`: Select / Confirm
- `Tab`: Next field / Next pane
- `Shift+Tab`: Previous field / Previous pane
- `←` / `→`: Resize panes (in multi-view)

### Level Management

- `E`: Edit selected level
- `D`: Delete selected level
- `S`: Save form
- `C`: Cancel order (in orders view)

### Multi-View

- `Tab`: Switch focus between panes
- `←` / `→`: Resize panes horizontally
- `↑` / `↓`: Resize panes vertically
- `+`: Add pane
- `-`: Remove pane

## Error Handling

### 1. Connection Errors

- **Automatic Reconnection**: Retry with exponential backoff
- **Visual Indicator**: Show connection status in status bar
- **Error Messages**: Display clear error messages
- **Graceful Degradation**: Show cached data if connection fails

### 2. Validation Errors

- **Real-Time Validation**: Validate fields as user types
- **Visual Feedback**: Highlight invalid fields
- **Error Messages**: Show specific error messages
- **Prevent Submission**: Disable save if form is invalid

### 3. Data Errors

- **Error Logging**: Log errors for debugging
- **User-Friendly Messages**: Show user-friendly error messages
- **Retry Mechanisms**: Allow retry for transient errors
- **Fallback Data**: Show cached data if fresh data unavailable

## Testing Strategy

### 1. Unit Tests

- **Widget Rendering**: Test individual widgets
- **Form Validation**: Test form validation logic
- **State Management**: Test state updates
- **Color Coding**: Test orderbook color coding logic

### 2. Integration Tests

- **gRPC Communication**: Test gRPC client/server interaction
- **Data Updates**: Test real-time data updates
- **Navigation**: Test navigation flow
- **Error Handling**: Test error scenarios

### 3. UI Tests

- **Screen Rendering**: Test all screens render correctly
- **Keyboard Navigation**: Test keyboard shortcuts
- **Color Schemes**: Test different color schemes
- **Multi-View**: Test split-screen functionality

### 4. Performance Tests

- **Rendering Performance**: Test rendering speed
- **Update Frequency**: Test update handling
- **Memory Usage**: Test memory consumption
- **Network Efficiency**: Test gRPC efficiency

## Deployment

### 1. Build Configuration

```toml
# cli/Cargo.toml
[package]
name = "trading-cli"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.26"
crossterm = "0.28"
tokio = { workspace = true }
tonic = { workspace = true }
prost = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
rust_decimal = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

# Workspace dependencies
trading_engine = { path = "../trading_engine" }
core_logic = { path = "../core_logic" }
```

### 2. Installation

```bash
# Build CLI
cargo build --release --bin trading-cli

# Install globally (optional)
cargo install --path src/cli --bin trading-cli
```

### 3. Configuration File

```json
{
  "grpc_endpoint": "http://localhost:50051",
  "refresh_interval_ms": 2000,
  "auto_refresh": true,
  "color_scheme": {
    "our_bid_color": "green",
    "our_ask_color": "red",
    "market_order_color": "white"
  },
  "theme": {
    "markers": "arrows",
    "show_quantity_breakdown": true
  }
}
```

### 4. Usage

```bash
# Start CLI
trading-cli

# With custom config
trading-cli --config /path/to/config.json

# With custom endpoint
trading-cli --endpoint http://localhost:50051
```

## Migration Plan

### Phase 1: Foundation (Week 1)

1. Set up CLI project structure
2. Implement gRPC client
3. Create basic navigation system
4. Implement main menu

### Phase 2: Level Management (Week 2)

1. Implement level setup menu
2. Create level list view
3. Implement create/edit forms
4. Add delete/disable functionality

### Phase 3: Trading Monitoring (Week 3)

1. Implement trading monitoring menu
2. Create dashboard view
3. Implement orderbook widget with color coding
4. Create balances, orders, trades, positions views
5. Implement risk metrics view

### Phase 4: System Monitoring (Week 4)

1. Implement system monitoring menu
2. Create system health view
3. Create protocol health view
4. Add connection status view
5. Implement performance metrics view

### Phase 5: Enhancements (Week 5)

1. Implement multi-view (split screen)
2. Add real-time streaming
3. Implement color schemes and themes
4. Add keyboard shortcuts
5. Performance optimization

### Phase 6: Polish (Week 6)

1. Error handling refinement
2. User experience improvements
3. Documentation
4. Testing
5. Deployment preparation

## Success Metrics

### User Experience

- **Navigation**: Intuitive, < 3 clicks to any feature
- **Response Time**: < 100ms for screen transitions
- **Update Latency**: < 2s for real-time updates
- **Error Rate**: < 1% user-facing errors

### Performance

- **Rendering**: 60 FPS smooth rendering
- **Memory**: < 100MB memory usage
- **CPU**: < 10% CPU usage when idle
- **Network**: Efficient gRPC usage

### Functionality

- **Feature Completeness**: 100% of planned features
- **Reliability**: 99.9% uptime
- **Usability**: Intuitive for technical users
- **Visual Clarity**: Clear color coding and markers

## Future Enhancements

### 1. Advanced Visualizations

- **Charts**: P&L charts, position history
- **Heatmaps**: Order density heatmaps
- **Animations**: Smooth transitions, order fill animations

### 2. Advanced Features

- **Search/Filter**: Search levels, filter orders
- **Export**: Export data to CSV/JSON
- **Scripting**: Scriptable commands for automation
- **Plugins**: Plugin system for custom widgets

### 3. Multi-Exchange Support

- **Exchange Selection**: Switch between exchanges
- **Multi-Exchange View**: View multiple exchanges simultaneously
- **Exchange-Specific Features**: Exchange-specific optimizations

### 4. Collaboration Features

- **Remote Access**: SSH-based remote access
- **Shared Sessions**: Multiple users viewing same session
- **Audit Log**: Track all configuration changes

## Notes

- **Production-Ready**: This TUI is designed to be production-ready, not a prototype
- **User Experience**: Focus on intuitive navigation and visual clarity
- **Performance**: Optimized for real-time updates and smooth rendering
- **Extensibility**: Designed for future enhancements and features
- **Integration**: Seamlessly integrates with the trading system via gRPC

