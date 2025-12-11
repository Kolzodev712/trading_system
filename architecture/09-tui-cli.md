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
│   │   └── trading_monitor/
│   │       ├── mod.rs            # Trading monitor orchestrator
│   │       ├── panels.rs          # Panel rendering
│   │       ├── popups.rs          # Popup rendering
│   │       ├── forms.rs           # Form rendering
│   │       └── input.rs           # Input handling
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
- Current screen (MainMenu, TradingMonitor)
- Level selection and form state
- Real-time data cache

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
- Navigation to Trading Monitor
- System information

##### Trading Monitor (`trading_monitor/`)

**Purpose**: Comprehensive real-time trading monitor.

**Components**:
- **Panels** (`panels.rs`): Individual panel rendering
  - Orders panel: Open orders with level mapping
  - Balances panel: Account balances
  - Levels panel: Active levels with status
  - Indicators panel: Real-time indicator values
  - Trades panel: Recent trades

- **Popups** (`popups.rs`): Overlay popups
  - Level details popup: Full level information
  - Error popup: Error messages

- **Forms** (`forms.rs`): Input forms
  - Edit level form: Edit level configuration
  - Create level form: Create new level

- **Input Handling** (`input.rs`): Keyboard input handling
  - Navigation (↑/↓, Tab)
  - Level toggle (T)
  - Level details (L)
  - Edit level (E)
  - Create level (C)
  - Form navigation and editing

**Features**:
- Real-time updates via gRPC streaming
- Level selection and navigation
- Level toggle (enable/disable)
- Level details view
- Level editing
- Level creation
- Real-time data display

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

### Trading Monitor

**Real-Time Display**:
- Open orders (with level mapping and order types)
- Account balances
- Recent trades
- Indicator values (real-time from Indicator Engine)
- Active levels (with status, enabled state, active orders)

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
- **Indicator values**: From `IndicatorService` (direct calculator access)
- **Levels**: From `LevelManager` (MongoDB persistence)

**Note**: Positions removed from TUI (internal calculation, not exchange data).

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
- **Trading Monitor**: 
  - ↑/↓: Navigate levels
  - T: Toggle level
  - L: View level details
  - E: Edit level
  - C: Create level
  - Tab: Switch between panels
  - Esc: Close popup/form

### Real-Time Updates

- Monitoring data automatically updates via gRPC stream
- UI refreshes on data updates
- No manual refresh needed

