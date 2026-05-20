# Indicator Engine Architecture

**Version**: 0.1  
**Last Updated**: December 2024

## Overview

The Indicator Engine calculates and provides real-time technical indicator values (SMA, EMA, composite indicators) based on market data. It is a **caching-only** service (not a state manager) that provides fast lookup for trading decisions.

## Key Principle

Indicator Engine is a **caching service** - it calculates and caches indicator values but does not manage state or publish updates. It subscribes to market data and publishes indicator values via channels.

## Architecture

### Structure

```
src/indicator_engine/
├── src/
│   ├── service.rs                # IndicatorService - orchestrates indicator engine
│   ├── calculator.rs              # IndicatorCalculator - performs calculations
│   ├── subscriber.rs              # MarketDataSubscriber - subscribes to market data
│   ├── publisher.rs               # IndicatorPublisher - publishes indicator values
│   └── data/
│       └── indicator_value.rs    # IndicatorValue - indicator data structure
```

### Components

#### 1. IndicatorService (`service.rs`)

**Purpose**: Orchestrates the indicator engine.

**Key Responsibilities**:
- Manages indicator registration
- Coordinates calculator, subscriber, and publisher
- Provides API for indicator value retrieval
- MongoDB persistence (optional)

**Methods**:
- `new()` - Create new IndicatorService
- `register_indicator(config: IndicatorConfig)` - Register new indicator
- `get_value(indicator_id: &str) -> Option<IndicatorValue>` - Get current value
- `get_history(indicator_id: &str, limit: Option<usize>) -> Vec<IndicatorValue>` - Get historical values
- `get_values_for_pair(pair: &Pair) -> Vec<IndicatorValue>` - Get all values for pair
- `get_values_by_name(name: &str) -> Vec<IndicatorValue>` - Get all values by indicator name
- `get_value_by_pair_and_name(pair: &Pair, name: &str) -> Option<IndicatorValue>` - Get specific value
- `get_all_values() -> Vec<IndicatorValue>` - Get all current values
- `run()` - Start indicator calculation loop

#### 2. IndicatorCalculator (`calculator.rs`)

**Purpose**: Performs indicator calculations.

**Key Responsibilities**:
- Calculates indicator values from market data
- Manages indicator state and history
- Efficient pair-based and name-based lookups
- Stores historical values (up to 100 per indicator)

**Data Structures**:
- `indicators: HashMap<SmolStr, CacheEntry>` - Indicator cache by ID
- `pair_index: HashMap<Pair, Vec<SmolStr>>` - Indicator IDs by pair
- `name_index: HashMap<SmolStr, Vec<SmolStr>>` - Indicator IDs by name

**CacheEntry**:
- `current: Option<IndicatorValue>` - Current value
- `history: Vec<IndicatorValue>` - Historical values (up to 100)
- `config: IndicatorConfig` - Indicator configuration

**Methods**:
- `update(pair: &Pair, orderbook: &StructuredOrderbook, trade: Option<&StructuredPublicTrade>)` - Update indicators
- `register_indicator(config: IndicatorConfig)` - Register new indicator
- `get_value(indicator_id: &str) -> Option<IndicatorValue>` - Get current value
- `get_history(indicator_id: &str, limit: Option<usize>) -> Vec<IndicatorValue>` - Get history
- `get_values_for_pair(pair: &Pair) -> Vec<IndicatorValue>` - Get values for pair
- `get_values_by_name(name: &str) -> Vec<IndicatorValue>` - Get values by name

#### 3. MarketDataSubscriber (`subscriber.rs`)

**Purpose**: Subscribes to market data updates for indicator calculation.

**Responsibilities**:
- Subscribes to `MarketDataStateManager` updates via broadcast channels
- Handles `MarketDataUpdate::Orderbook` and `MarketDataUpdate::Trade`
- Triggers indicator calculations on market data updates
- Handles channel lag gracefully

#### 4. IndicatorPublisher (`publisher.rs`)

**Purpose**: Publishes indicator values via broadcast channels.

**Responsibilities**:
- Publishes indicator value updates via broadcast channels
- Trading Engine subscribes for trading decisions
- TUI receives via gRPC aggregation

## Supported Indicators

### Simple Indicators

- **SMA (Simple Moving Average)**: Average price over N periods
- **EMA (Exponential Moving Average)**: Weighted average with exponential decay

### Composite Indicators

- **Custom Composite**: Combine multiple indicators with custom logic
- Extensible framework for new indicators

## Data Flow

```
MarketDataStateManager (via broadcast channel) → Indicator Engine
    ├── MarketDataSubscriber receives updates
    ├── IndicatorCalculator calculates values
    ├── Update cache (current + history)
    └── IndicatorPublisher publishes values
        ├──→ Trading Engine (subscribes, caches for trading decisions)
        └──→ TUI (via gRPC aggregation)
```

## Communication

### Input
- **MarketDataStateManager** (via broadcast channel): Orderbooks, public trades

### Output
- **Broadcast Channels**: Indicator value updates published to subscribers
  - Trading Engine subscribes for trading decisions
  - TUI receives via gRPC aggregation

## Performance Characteristics

- **Lookup Latency**: < 0.1ms (in-memory cache)
- **Calculation Latency**: < 1ms per indicator
- **Memory**: Efficient data structures (SmolStr, HashMap indexes)
- **History Storage**: Up to 100 values per indicator

## Caching Strategy

### In-Memory Cache

- **Current Values**: Stored in `CacheEntry.current`
- **Historical Values**: Stored in `CacheEntry.history` (up to 100)
- **Indexes**: Fast lookups by pair and name
  - `pair_index`: HashMap<Pair, Vec<IndicatorId>>
  - `name_index`: HashMap<Name, Vec<IndicatorId>>

### Cache Updates

- **Real-time**: Updates on every market data update
- **Incremental**: Only recalculates affected indicators
- **History Management**: FIFO queue (removes oldest when limit reached)

## Persistence

### MongoDB (Optional)

**Purpose**: Indicator configuration persistence

**Status**: Optional, can be enabled for persistence

**Collections**:
- `indicators` - Indicator definitions

## Performance Optimizations

- **SmolStr**: Efficient string storage
- **HashMap Indexes**: Fast pair-based and name-based lookups
- **Incremental Updates**: Only recalculates when market data changes
- **History Limit**: Prevents unbounded memory growth

## Dependencies

- `trading_core`: Types, channels
- `tokio`: Async runtime
- `rust_decimal`: Decimal arithmetic

## Testing

- Unit tests for calculator (39 tests)
- Unit tests for service
- Integration tests for end-to-end workflows

## Configuration

### Indicator Configuration

```json
{
  "indicators": [
    {
      "id": "sma_btc_20",
      "name": "SMA",
      "pair": {"base": "BTC", "quote": "USDT"},
      "period": 20,
      "source": "close"
    }
  ]
}
```

## Configuration sources

Indicators and level pricing are defined outside the engine and registered at startup (see `src/indicator_engine/src/config.rs`, `level_pricing.rs`, `service.rs`).

| Source | Contents |
|--------|----------|
| MongoDB `indicators` collection (optional) | Indicator docs + `level_pricing` (`indicator_type: "level_pricing"`) |
| `config/indicators.json` | `version`, `indicators[]`, `level_pricing[]` |

**Load order**: Mongo first, then JSON file. Duplicate indicator **ids**: Mongo wins. If Mongo already registered indicators, the file is not used to register more indicators; file `level_pricing` may still apply when Mongo has no pricing rules.

**Build-time generation** (from `python/`):

```bash
cd python && uv sync
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json
uv run indicator-validate --config ../config/indicators.json
```

Simple indicators use ids `{name}_{period}_{pair}` (e.g. `sma_20_BTCUSDT`). Levels reference pricing via `"price": "Binding"` in `config/levels.json` and matching `level_id` in `level_pricing`; `indicator_ref` may be pair-agnostic (`sma_20`) and is resolved per pair in the trading engine.

## Error Handling

- **Channel Lag**: Handled gracefully with warnings
- **Calculation Errors**: Logged, indicator value set to None
- **Invalid Config**: Validation before registration

## Extensibility

### Adding New Indicators

1. Implement calculation logic in `IndicatorCalculator`
2. Add indicator type to `IndicatorConfig`
3. Register indicator via `IndicatorService::register_indicator()`

### Example: Adding RSI Indicator

```rust
// In calculator.rs
match config.name.as_str() {
    "SMA" => calculate_sma(...),
    "EMA" => calculate_ema(...),
    "RSI" => calculate_rsi(...),  // New indicator
    _ => return Err(...),
}
```

