# Market Data Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Market Data Engine manages public market data (orderbooks, public trades) received from the exchange via WebSocket. It provides this data to other components through broadcast channels for indicator calculations and trading decisions.

## Key Principle

Market Data Engine is the **single source of truth** for public market data. All components that need market data subscribe to its broadcast channels.

## Architecture

### Structure

```
src/market_data_engine/
├── src/
│   ├── state/
│   │   ├── state.rs              # MarketDataState - in-memory state
│   │   ├── manager.rs             # MarketDataStateManager - orchestrates state
│   │   └── publisher.rs          # MarketDataStatePublisher - publishes updates
│   └── driver.rs                 # MarketDataDriver - WebSocket driver
```

### Components

#### 1. MarketDataState (`state.rs`)

**Purpose**: In-memory state structure holding all market data.

**Data Structures**:
- `orderbooks: HashMap<Pair, StructuredOrderbook>` - Orderbook snapshots by pair
- `public_trades: HashMap<Pair, Vec<StructuredPublicTrade>>` - Public trade history by pair
- `symbol_registry: SymbolRegistry` - For symbol↔pair conversions

**Key Features**:
- Uses `SymbolRegistry` for reliable `Symbol` ↔ `Pair` conversions
- Efficient pair-based lookups
- Thread-safe access via `Arc<RwLock<MarketDataState>>`

**Methods**:
- `get_orderbook(pair: &Pair) -> Option<&StructuredOrderbook>` - Get orderbook for pair
- `get_public_trades(pair: &Pair) -> Vec<StructuredPublicTrade>` - Get trades for pair
- `get_pairs_with_orderbooks() -> Vec<Pair>` - Get all pairs with orderbooks

#### 2. MarketDataStateManager (`manager.rs`)

**Purpose**: Orchestrates state updates and publishing.

**Key Responsibilities**:
- Manages in-memory state: orderbooks, public trades
- Uses `SymbolRegistry` for symbol↔pair conversions
- Publishes all updates via broadcast channels to:
  - Indicator Engine (subscribes, updates indicator cache)
  - Trading Engine (for level execution)
  - TUI (via gRPC aggregation)
- Updates include `Pair` information for proper routing
- MongoDB persistence capability (disabled initially, lazy evaluation)

**Methods**:
- `update_orderbook(symbol: &Symbol, orderbook: StructuredOrderbook)` - Update orderbook
- `add_public_trade(symbol: &Symbol, trade: StructuredPublicTrade)` - Add public trade
- `get_state() -> Arc<RwLock<MarketDataState>>` - Get state for reading

#### 3. MarketDataStatePublisher (`publisher.rs`)

**Purpose**: Publishes state updates via broadcast channels.

**Responsibilities**:
- Publishes all updates via broadcast channels
- Updates include `Pair` information for proper routing
- Uses broadcast channels for efficient fan-out
- Each subscriber tracks its own lag

## Data Flow

```
Exchange WebSocket → MarketDataStateManager
    ├── Update in-memory state (using SymbolRegistry for conversions)
    ├── Publish via Broadcast Channel (with Pair information)
    │   ├──→ Indicator Engine (subscribes, updates indicator cache)
    │   ├──→ Trading Engine (for level execution)
    │   └──→ TUI (via gRPC aggregation)
    └── Persist to MongoDB* (if enabled)
```

## Communication

### Input
- **WebSocket**: Real-time market data from exchange (orderbooks, public trades)

### Output
- **Broadcast Channels**: Market data updates published to subscribers
  - Indicator Engine subscribes for indicator calculations
  - Trading Engine subscribes for trading decisions
  - TUI receives via gRPC aggregation

## WebSocket Message Processing

### Orderbook Updates
- Snapshot or incremental updates
- Updates `MarketDataState.orderbooks`
- Publishes `MarketDataUpdate::Orderbook` via channel

### Public Trade Updates
- Individual trade events
- Appends to `MarketDataState.public_trades`
- Publishes `MarketDataUpdate::Trade` via channel

## Reconciliation

**Not Required**: Market data is public and doesn't require reconciliation. The exchange is the authoritative source, and we simply consume the data stream.

## Persistence

### MongoDB (Optional)

**Purpose**: Historical data storage (optional)

**Status**: Disabled initially, lazy evaluation

**Note**: Market data is typically not persisted as it's high-frequency and can be regenerated from exchange history if needed.

## Performance Characteristics

- **Latency**: < 1μs for channel publishing
- **Throughput**: 100+ updates/second (orderbooks), 1000+ updates/second (trades)
- **Memory**: Efficient in-memory data structures
- **Lookup**: O(1) for orderbook retrieval by pair

## Dependencies

- `trading_core`: Types, channels, SymbolRegistry
- `tokio`: Async runtime
- Exchange-specific market data clients

## Testing

- Unit tests for state management
- Integration tests for channel publishing
- WebSocket message processing tests

## Configuration

### Channel Configuration

Channel buffer sizes configured via `ChannelConfig`:
- `market_data_capacity`: Broadcast channel capacity (default: 1000)
- `market_data_engine_buffer`: Internal buffer for exchange-specific clients (default: 1000)

## Error Handling

- **WebSocket Disconnections**: Automatic reconnection with exponential backoff
- **Channel Lag**: Subscribers track lag independently
- **Invalid Data**: Validated before state update

## Security

- **Public Data**: No authentication required (public market data)
- **TLS/SSL**: All connections use TLS
- **Rate Limiting**: Handled automatically by exchange clients

