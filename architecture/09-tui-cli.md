# TUI CLI Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The TUI (Terminal User Interface) CLI is an external process that provides a terminal-based interface for trading operations and monitoring. It connects to the Trading System via gRPC and provides real-time monitoring, level management, and trading control.

## Key Principle

**TUI IS the monitoring interface** - no separate monitoring service needed. It provides comprehensive real-time monitoring and level management capabilities.

## Architecture

### Structure

```
cli/
├── src/
│   ├── main.rs                   # Application entry point
│   ├── app.rs                    # Application state management
│   ├── client.rs                 # gRPC client
│   ├── screens/
│   │   ├── mod.rs                # Screen orchestrator
│   │   ├── main_menu.rs          # Main menu screen
│   │   ├── shared/               # Shared components
│   │   │   ├── mod.rs            # Shared module
│   │   │   ├── panels.rs          # Shared panel rendering
│   │   │   ├── popups.rs          # Shared popup rendering
│   │   │   ├── forms.rs           # Shared form rendering
│   │   │   └── input.rs           # Shared input handling
│   │   ├── spot_monitor/
│   │   │   └── mod.rs            # SpotMonitorScreen
│   │   └── futures_monitor/
│   │       ├── mod.rs            # FuturesMonitorScreen
│   │       └── panels.rs          # Futures-specific panels
│   └── error.rs                  # Error handling
```

### Components

#### 1. Application Entry Point (`main.rs`)

**Purpose**: Application entry point that initializes TUI and starts event loop.

**Responsibilities**:
- Parse command-line arguments (`--endpoint`)
- Initialize gRPC client
- Initialize TUI with `ratatui`
- Start event loop
- Handle graceful shutdown

#### 2. Application State (`app.rs`)

**Purpose**: Manages application state and coordinates screens.

**State Management**:
- `App` struct holds all application state
- Current screen (MainMenu, SpotMonitor, FuturesMonitor)
- Level selection and form state
- Real-time data cache
- Automatic screen switching based on `instrument_type` from monitoring data

**Key Methods**:
- `new()` - Create new App
- `run()` - Start application event loop
- `handle_input()` - Handle keyboard input
- `update_data()` - Update from gRPC stream

#### 3. gRPC Client (`client.rs`)

**Purpose**: gRPC client for communication with Trading System.

**Responsibilities**:
- Connect to gRPC server
- Stream monitoring data
- Send level management requests
- Send trading control requests

**Methods**:
- `connect(endpoint: &str) -> Result<Client>`
- `stream_monitoring_data() -> Result<Stream>`
- `get_levels() -> Result<Vec<Level>>`
- `create_level(config: LevelConfig) -> Result<()>`
- `update_level(id: &str, config: LevelConfig) -> Result<()>`
- `toggle_level(id: &str) -> Result<()>`
- `set_trading_enabled(enabled: bool) -> Result<()>`

#### 4. Screens (`screens/`)

##### Main Menu (`main_menu.rs`)

**Purpose**: Main navigation screen.

**Features**:
- Trading status display
- Navigation to Spot Monitor or Futures Monitor (auto-detected based on instrument type)
- System information

##### Spot Monitor (`spot_monitor/`)

**Purpose**: Real-time spot trading monitor.

**Components**:
- Uses shared components from `shared/` module
- **Layout**:
  - Orders panel: Open orders with level mapping
  - Balances panel: Account balances
  - Levels/Strategy panel: Active levels (levels mode) or strategy status (strategies mode)
  - Orderbook panel: Real-time orderbook data
  - Indicators panel: Real-time indicator values
  - Trades panel: Recent trades

**Features**:
- Real-time updates via gRPC streaming
- **Levels Mode**:
  - Level selection and navigation
  - Level toggle (enable/disable)
  - Level details view
  - Level editing
  - Level creation
- **Strategies Mode**:
  - Strategy selection and navigation (↑/↓ keys)
  - Per-strategy status display (Running/Paused for each strategy)
  - Position and P&L monitoring per strategy
  - Active orders tracking per strategy
  - Per-strategy trading control (enable/disable individual strategies)
  - Strategy details view (L key) - popup overlay with full strategy information

##### Futures Monitor (`futures_monitor/`)

**Purpose**: Real-time futures trading monitor with position-focused display.

**Components**:
- **Futures-Specific Panels** (`panels.rs`):
  - Positions panel: Exchange-level positions with leverage, margin, P&L, liquidation price
  - Margin info panel: Collateral, margin used, total P&L, position count
- **Shared Panels** (from `shared/`):
  - Orders panel: Open orders
  - Orderbook panel: Real-time orderbook data
  - Indicators panel: Real-time indicator values
  - Trades panel: Recent trades

**Features**:
- Real-time position updates from exchange WebSocket
- Position display with:
  - Side (Long/Short) with color coding
  - Entry price, mark price
  - Unrealized P&L with color coding
  - Leverage
  - Liquidation price
- Margin information:
  - Collateral balance
  - Total margin used
  - Total unrealized P&L
  - Position count

##### Shared Components (`shared/`)

**Purpose**: Common components used by both Spot and Futures monitors.

**Components**:
- **Panels** (`panels.rs`): Shared panel rendering
  - Orders panel
  - Orderbook panel
  - Indicators panel
  - Trades panel
  - Balances panel (spot-specific)
  - Levels panel (spot-specific)
  - Strategy panel (spot-specific)
- **Popups** (`popups.rs`): Overlay popups
  - Level details popup
  - Strategy details popup
  - Error popup
- **Forms** (`forms.rs`): Input forms
  - Edit level form
  - Create level form
- **Input Handling** (`input.rs`): Keyboard input handling
  - Navigation (↑/↓, Tab)
  - Level toggle (T) - in levels mode
  - Trading toggle (T) - in strategies mode
  - Level details (L) - in levels mode
  - Edit level (E) - in levels mode
  - Create level (C) - in levels mode
  - Form navigation and editing

## Data Flow

### Real-Time Updates Flow

```
gRPC Stream → TUI Client
    ├── Parse monitoring data
    ├── Update application state
    └── Trigger UI refresh
        └── Render updated panels
```

### Level Management Flow

```
User Input → TUI
    ├── Parse input (keyboard)
    ├── Send gRPC request
    ├── Receive response
    └── Update UI
```

## Communication

### Input
- **Keyboard**: User input for navigation and actions
- **gRPC Stream**: Real-time monitoring data updates

### Output
- **Terminal**: TUI rendering via `ratatui`
- **gRPC Requests**: Level management and trading control

## Features

### Main Menu

- Trading status display
- Navigation options
- System information

### Spot Monitor

**Real-Time Display**:
- Open orders (with level mapping and order types)
- Account balances
- Recent trades
- Indicator values (real-time from Indicator Engine)
- Active levels (with status, enabled state, active orders) or strategy status

### Futures Monitor

**Real-Time Display**:
- Exchange-level positions (with leverage, margin, P&L, liquidation price)
- Margin information (collateral, margin used, total P&L)
- Open orders
- Orderbook data
- Indicator values
- Recent trades

**Level Management**:
- Level selection and navigation (↑/↓ keys)
- Level toggle (T key) - enable/disable levels with automatic order cancellation
- Level details view (L key) - popup overlay with full level information
- Level editing (E key) - edit level configuration (price, quantity, risk limits, lifecycle settings)
- Level creation (C key) - create new levels with symbol selection from existing levels

**Real-Time Updates**:
- gRPC streaming for live data updates
- Automatic UI refresh on data updates
- Smooth rendering with `ratatui`

## Technical Implementation

### Framework

- **TUI Rendering**: `ratatui` for terminal UI
- **gRPC Client**: `tonic` for communication
- **Protocol**: Protocol Buffers (`.proto` files in `libs/trading_core/proto/`)

### State Management

- **Local State**: Application state in `App` struct
- **Real-Time Updates**: Updates from gRPC stream
- **Form State**: Temporary state for forms

### Error Handling

- **Comprehensive Error Handling**: Graceful degradation
- **Error Display**: Error popups for user feedback
- **Logging**: File-based logging to `cli_debug.log` (doesn't clutter terminal)

## gRPC Service Methods

### Trading Control
- `SetTradingEnabled` / `GetTradingStatus` - Enable/disable trading

### Level Management
- `GetLevels` / `GetLevel` - Level retrieval
- `CreateLevel` / `UpdateLevel` / `ToggleLevel` - Level management

### Monitoring
- `GetMonitoringData` / `StreamMonitoringData` - Real-time monitoring

## Data Sources

All data fetched from existing execution layer (no separate REST calls):
- **Balances, orders, trades**: From `ExchangeStateManager` (WebSocket updates)
- **Positions**: From `ExchangeStateManager` (WebSocket updates for futures)
- **Indicator values**: From `IndicatorService` (direct calculator access)
- **Levels**: From `LevelManager` (MongoDB persistence)

**Screen Selection**:
- TUI automatically detects `instrument_type` from `MonitoringData`
- Switches to `FuturesMonitor` for futures trading
- Switches to `SpotMonitor` for spot trading

## Performance Characteristics

- **Rendering**: 60 FPS (limited by terminal)
- **Update Latency**: < 100ms (gRPC streaming)
- **Memory**: Efficient state management
- **CPU**: Low usage (event-driven)

## Dependencies

- `ratatui`: TUI framework
- `tonic`: gRPC client
- `tokio`: Async runtime
- `trading_core`: Protocol definitions

## Testing

- Manual testing for UI interactions
- Integration tests for gRPC communication
- Unit tests for state management

## Configuration

### gRPC Endpoint

```bash
./trading-cli --endpoint http://localhost:50051
```

Default: `http://localhost:50051`

## Error Handling

- **Connection Errors**: Automatic reconnection with exponential backoff
- **gRPC Errors**: Displayed in error popup
- **Input Errors**: Validation and user feedback
- **Rendering Errors**: Graceful degradation

## Security

- **gRPC**: Can add authentication if needed
- **Local State**: No sensitive data stored
- **Logging**: File-based logging (doesn't expose secrets)

## Scripts

### Dependency Checking

- `scripts/check_dependencies.sh` - Pre-flight check for dependencies and ports
  - Checks MongoDB connection
  - Checks Redis (if enabled)
  - Verifies ports are free (gRPC: 50051, Health: 8080)
  - Auto-starts missing dependencies in new terminal windows
  - Supports systemd, Docker, and direct execution

### Port Management

- `scripts/kill_grpc_port.sh` - Utility to free gRPC port if occupied

## Usage

### Starting TUI CLI

```bash
# From project root
cargo run --release --bin trading-cli -- --endpoint http://localhost:50051
```

### Navigation

- **Main Menu**: Navigate with arrow keys, Enter to select
- **Spot Monitor**: 
  - ↑/↓: Navigate levels/strategies
  - T: Toggle level/strategy
  - L: View level/strategy details
  - E: Edit level (levels mode only)
  - C: Create level (levels mode only)
  - Tab: Switch between panels
  - Esc: Close popup/form
- **Futures Monitor**:
  - ↑/↓: Navigate positions
  - Tab: Switch between panels
  - Esc: Back to main menu

### Real-Time Updates

- Monitoring data automatically updates via gRPC stream
- UI refreshes on data updates
- No manual refresh needed

