# Proposal 03: Redis to Channels Migration

## Overview

This proposal outlines the migration from Redis-based communication to in-process channels for all internal monolith communication. Since the system is a monolith where all services (Exchange Engine, Market Data Engine, Trading Engine, Indicator Service) live in the same workspace, we can use ultra-low latency channels (< 1μs) instead of Redis pub/sub for internal communication.

**Prerequisite**: This migration should be performed after **Proposal 03 (Level-Based Trading System)** is implemented. Level-based trading can work with Redis initially; the migration to channels is a performance optimization that can be done once the core trading functionality is stable.

**Note**: After channel migration is complete, the hardcoded indicator calculation logic in `trading_engine/src/strategies.rs` (BuyLowSellHigh strategy) should be removed. The strategy will then subscribe to indicator updates via channels instead of calculating moving averages directly.

## Current State

### Current Architecture (Redis-Based)

```
Market Data Engine → Redis Pub/Sub → Trading Engine
```

**Current Flow**:
1. Market Data Engine receives orderbook/trade updates from Binance WebSocket
2. Market Data Engine publishes to Redis channels (`orderbook`, `trade`)
3. Trading Engine subscribes to Redis channels
4. Trading Engine receives updates and processes them

**Current Implementation**:
- `market_data_engine/src/messages.rs`: `RedisMessageHandler` publishes to Redis
- `trading_engine/src/executor.rs`: Subscribes to Redis channels
- `core_logic/src/redis.rs`: Redis client wrapper

## Target Architecture (Channel-Based)

### Target Flow

```
Market Data Engine → Channel → Trading Engine
Exchange Engine → Channel → Trading Engine
Indicator Service → Channel → Trading Engine
```

**Benefits**:
- **Ultra-low latency**: < 1μs vs ~1-5ms for Redis
- **No serialization overhead**: Direct memory access
- **No external dependency**: No Redis required for internal communication
- **Simpler deployment**: One less service to manage
- **Better performance**: Critical for high-frequency trading

## Key Requirements

1. **Remove Redis dependency** from internal communication
2. **Create channel types** in `core_logic` for all internal communication
3. **Update Market Data Engine** to publish via channels instead of Redis
4. **Update Trading Engine** to receive via channels instead of Redis
5. **Update Exchange Engine** to publish via channels (if not already)
6. **Keep Redis code** behind feature flag for potential future Web UI use
7. **Update Runner** to create channel pairs and wire services together
8. **Remove hardcoded indicator logic** from Trading Engine after channels are in place (Trading Engine will subscribe to indicator updates via channels)
9. **Support multiple consumers** via broadcast channels for fan-out scenarios (e.g., Market Data → Indicator Engine, Trading Engine, DB, Risk Service)

## Component Structure

### Channel Types in `core_logic`

```
core_logic/
├── src/
│   ├── channels/
│   │   ├── mod.rs
│   │   ├── exchange.rs          # Exchange update channels
│   │   ├── market_data.rs       # Market data update channels
│   │   └── indicators.rs        # Indicator update channels
```

### Updated Services

```
market_data_engine/
├── src/
│   ├── messages.rs              # Update: Channel publisher instead of Redis
│   └── driver.rs                # Update: Accept channel sender

trading_engine/
├── src/
│   ├── executor.rs              # Update: Receive from channels instead of Redis
│   └── channels/
│       ├── mod.rs
│       ├── exchange_subscriber.rs
│       ├── market_data_subscriber.rs
│       └── indicator_subscriber.rs

runner/
├── src/
│   └── main.rs                  # Update: Create channel pairs and wire services
```

## Detailed Component Design

### 1. Channel Types (`core_logic/src/channels/mod.rs`)

```rust
pub mod exchange;
pub mod market_data;
pub mod indicators;

pub use exchange::{ExchangeUpdateSender, ExchangeUpdateReceiver, ExchangeUpdate};
pub use market_data::{MarketDataUpdateSender, MarketDataUpdateReceiver, MarketDataUpdate};
pub use indicators::{IndicatorUpdateSender, IndicatorUpdateReceiver, IndicatorUpdate};
```

### 2. Market Data Channels (`core_logic/src/channels/market_data.rs`)

```rust
use tokio::sync::mpsc;
use crate::data::public_data::{Orderbook, Trade};

pub type MarketDataUpdateSender = mpsc::UnboundedSender<MarketDataUpdate>;
pub type MarketDataUpdateReceiver = mpsc::UnboundedReceiver<MarketDataUpdate>;

#[derive(Debug, Clone)]
pub enum MarketDataUpdate {
    Orderbook(Orderbook),
    Trade(Trade),
}
```

### 3. Exchange Channels (`core_logic/src/channels/exchange.rs`)

```rust
use tokio::sync::mpsc;
use crate::data::private_data::{Order, Balance, Position, Trade};

pub type ExchangeUpdateSender = mpsc::UnboundedSender<ExchangeUpdate>;
pub type ExchangeUpdateReceiver = mpsc::UnboundedReceiver<ExchangeUpdate>;

#[derive(Debug, Clone)]
pub enum ExchangeUpdate {
    Order(Order),
    Balance(Balance),
    Position(Position),
    Trade(Trade),
}
```

### 4. Indicator Channels (`core_logic/src/channels/indicators.rs`)

```rust
use tokio::sync::mpsc;
use rust_decimal::Decimal;
use std::collections::HashMap;

pub type IndicatorUpdateSender = mpsc::UnboundedSender<IndicatorUpdate>;
pub type IndicatorUpdateReceiver = mpsc::UnboundedReceiver<IndicatorUpdate>;

#[derive(Debug, Clone)]
pub struct IndicatorUpdate {
    pub name: String,
    pub value: Decimal,
    pub timestamp: u64,
    pub metadata: HashMap<String, String>,
}
```

### 5. Updated Market Data Message Handler (`market_data_engine/src/messages.rs`)

```rust
use async_trait::async_trait;
use core_logic::{
    data::public_data::{Orderbook, Trade},
    error::EngineError,
    channels::market_data::{MarketDataUpdateSender, MarketDataUpdate},
};
use std::fmt::Debug;
use tracing::debug;

/// Trait that defines how messages (orderbooks, trades) should be processed.
#[async_trait]
pub trait WSMessageHandler: Send + Sync + Debug {
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError>;
    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError>;
}

/// **Handles incoming WebSocket messages and publishes them via channels**
#[derive(Debug)]
pub struct ChannelMessageHandler {
    market_data_tx: MarketDataUpdateSender,
}

impl ChannelMessageHandler {
    pub fn new(market_data_tx: MarketDataUpdateSender) -> Self {
        Self { market_data_tx }
    }
}

#[async_trait]
impl WSMessageHandler for ChannelMessageHandler {
    async fn handle_orderbook(&self, orderbook: Orderbook) -> Result<(), EngineError> {
        self.market_data_tx
            .send(MarketDataUpdate::Orderbook(orderbook))
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!(
                "Failed to send orderbook to channel: {error}",
                error = e
            )))?;

        debug!("Published orderbook to channel.");
        Ok(())
    }

    async fn handle_trade(&self, trade: Trade) -> Result<(), EngineError> {
        self.market_data_tx
            .send(MarketDataUpdate::Trade(trade))
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!(
                "Failed to send trade to channel: {error}",
                error = e
            )))?;

        debug!("Published trade to channel.");
        Ok(())
    }
}

// Keep Redis handler behind feature flag for future Web UI
#[cfg(feature = "redis")]
pub struct RedisMessageHandler {
    redis_client: RedisClient,
}

#[cfg(feature = "redis")]
#[async_trait]
impl WSMessageHandler for RedisMessageHandler {
    // ... existing Redis implementation
}
```

### 6. Updated Market Data Driver (`market_data_engine/src/driver.rs`)

```rust
use super::{config::MarketDataConfig, messages::ChannelMessageHandler};
use crate::{config::ExchangeConfig, exchanges::binance::client::BinanceClient};
use core_logic::{
    error::EngineError,
    channels::market_data::MarketDataUpdateSender,
    traits::MarketDataClient,
};
use tracing::error;

pub struct MarketDataDriver {
    exchange_client: Box<dyn MarketDataClient>,
}

impl MarketDataDriver {
    pub fn new(
        config: MarketDataConfig,
        market_data_tx: MarketDataUpdateSender,
    ) -> Result<Self, EngineError> {
        let exchange_client = Self::create_exchange_client(&config, market_data_tx)?;

        Ok(Self { exchange_client })
    }

    pub fn create_exchange_client(
        config: &MarketDataConfig,
        market_data_tx: MarketDataUpdateSender,
    ) -> Result<Box<dyn MarketDataClient>, EngineError> {
        match &config.exchange_config {
            ExchangeConfig::Binance(binance_config) => {
                let message_handler = Box::new(ChannelMessageHandler::new(market_data_tx));
                Ok(Box::new(BinanceClient::new(
                    binance_config.clone(),
                    100,
                    Some(message_handler),
                )))
            }
        }
    }

    pub async fn run(&mut self, method: Method) -> Result<(), EngineError> {
        // ... existing implementation
    }
}
```

### 7. Updated Trading Engine Executor (`trading_engine/src/executor.rs`)

```rust
use core_logic::channels::market_data::{MarketDataUpdateReceiver, MarketDataUpdate};

impl StrategyExecutor {
    pub async fn run_with_channels(
        &mut self,
        mut market_data_rx: MarketDataUpdateReceiver,
        // ... other channels
    ) -> Result<(), EngineError> {
        loop {
            tokio::select! {
                // Receive market data updates from channel
                update = market_data_rx.recv() => {
                    match update {
                        Some(MarketDataUpdate::Orderbook(orderbook)) => {
                            // Process orderbook update
                            let order_request = {
                                let mut strat = self.strategy.write().await;
                                let state = strat.get_state().clone();
                                match strat.on_orderbook(&orderbook, &state).await {
                                    Ok(req) => req,
                                    Err(e) => {
                                        error!("Strategy error in on_orderbook: {error}", error = e);
                                        continue;
                                    }
                                }
                            };

                            // Place order if strategy requested
                            if let Some(mut req) = order_request {
                                // ... existing order placement logic
                            }
                        }
                        Some(MarketDataUpdate::Trade(trade)) => {
                            // Process trade update if needed
                            debug!("Received trade update: {trade:?}");
                        }
                        None => {
                            warn!("Market data channel closed");
                            break;
                        }
                    }
                }
                // ... other channel receivers
            }
        }
    }
}
```

### 8. Updated Runner (`runner/src/main.rs`)

```rust
use core_logic::channels::{
    market_data::{market_data_channel, MarketDataUpdateSender, MarketDataUpdateReceiver},
    exchange::{exchange_channel, ExchangeUpdateSender, ExchangeUpdateReceiver},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ... existing setup

    // Create channel pairs
    let (market_data_tx, market_data_rx) = market_data_channel();
    let (exchange_tx, exchange_rx) = exchange_channel();

    // Start Market Data Engine with channel sender
    let market_data_config = MarketDataConfig::from_config(&config)?;
    let mut market_data_driver = MarketDataDriver::new(market_data_config, market_data_tx)?;
    
    let market_data_handle = tokio::spawn(async move {
        market_data_driver.run(Method::Orderbooks).await
    });

    // Start Exchange Engine with channel sender
    // ... (similar pattern)

    // Start Trading Engine with channel receivers
    let mut executor = StrategyExecutor::new(
        strategy,
        trading_client,
        // ... other params
    );

    let executor_handle = tokio::spawn(async move {
        executor.run_with_channels(market_data_rx, exchange_rx).await
    });

    // Wait for shutdown
    // ...
}
```

### 9. Helper Functions (`core_logic/src/channels/mod.rs`)

```rust
pub mod market_data {
    use super::*;

    pub fn market_data_channel() -> (MarketDataUpdateSender, MarketDataUpdateReceiver) {
        mpsc::unbounded_channel()
    }
}

pub mod exchange {
    use super::*;

    pub fn exchange_channel() -> (ExchangeUpdateSender, ExchangeUpdateReceiver) {
        mpsc::unbounded_channel()
    }
}
```

## Migration Plan

### Phase 1: Create Channel Infrastructure

1. **Create channel types in `core_logic`**
   - Add `channels/` module
   - Define `MarketDataUpdate`, `ExchangeUpdate`, `IndicatorUpdate` types
   - Create channel type aliases

2. **Update `core_logic/Cargo.toml`**
   - Ensure `tokio` dependency includes `sync` feature

### Phase 2: Update Market Data Engine

1. **Create `ChannelMessageHandler`**
   - Replace `RedisMessageHandler` with channel-based handler
   - Keep Redis handler behind feature flag

2. **Update `MarketDataDriver`**
   - Accept `MarketDataUpdateSender` in constructor
   - Pass to message handler

3. **Remove Redis dependency** (or make it optional)
   - Update `Cargo.toml` to make `redis` dependency optional
   - Add feature flag for Redis support

### Phase 3: Update Trading Engine

1. **Create channel subscribers**
   - Add `channels/` module in `trading_engine`
   - Create subscriber types for each channel type

2. **Update `StrategyExecutor`**
   - Replace Redis subscription with channel receiver
   - Update `run()` method to use channels

3. **Remove Redis dependency** (or make it optional)
   - Update `Cargo.toml` to make `redis` dependency optional

### Phase 4: Update Runner

1. **Create channel pairs**
   - Create `(sender, receiver)` pairs for each channel type
   - Pass senders to engines, receivers to trading engine

2. **Wire services together**
   - Start all services with appropriate channel endpoints
   - Ensure proper shutdown handling

### Phase 5: Testing and Validation

1. **Unit tests**
   - Test channel message passing
   - Test channel closure handling

2. **Integration tests**
   - Test full flow: Market Data → Channel → Trading Engine
   - Verify no Redis dependency for internal communication

3. **Performance tests**
   - Measure latency: Channel vs Redis
   - Verify < 1μs latency for channels

### Phase 6: Cleanup

1. **Remove Redis code** (or keep behind feature flag)
   - Remove Redis client from internal communication paths
   - Keep Redis code for potential future Web UI use

2. **Update documentation**
   - Update README to reflect channel-based architecture
   - Update architecture diagrams

## Redis Feature Flag

### Cargo.toml Configuration

```toml
# In market_data_engine/Cargo.toml
[dependencies]
# ... other dependencies

[features]
default = []
redis = ["core_logic/redis"]  # Optional Redis support for future Web UI
```

### Usage

```rust
#[cfg(feature = "redis")]
use core_logic::redis::RedisClient;

#[cfg(feature = "redis")]
pub struct RedisMessageHandler {
    redis_client: RedisClient,
}
```

## Error Handling

### Channel Closure

```rust
match market_data_rx.recv().await {
    Some(update) => {
        // Process update
    }
    None => {
        warn!("Market data channel closed - service may have shut down");
        // Handle graceful shutdown
        break;
    }
}
```

### Channel Send Errors

```rust
market_data_tx.send(update).map_err(|e| {
    EngineError::CustomError(anyhow::anyhow!(
        "Failed to send update to channel: {error}",
        error = e
    ))
})?;
```

## Performance Considerations

### Channel Buffer Size

- **Unbounded channels**: Use for high-frequency updates (orderbooks, trades)
- **Bounded channels**: Use if backpressure is needed (not recommended for trading)

### Channel Type Selection

- **`mpsc::UnboundedSender/Receiver`**: Default choice for trading system
- **`mpsc::Sender/Receiver`**: Use only if backpressure is required

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_market_data_channel() {
        let (tx, mut rx) = market_data_channel();
        
        let orderbook = Orderbook::new(/* ... */);
        tx.send(MarketDataUpdate::Orderbook(orderbook.clone())).unwrap();
        
        let received = rx.recv().await.unwrap();
        match received {
            MarketDataUpdate::Orderbook(ob) => assert_eq!(ob, orderbook),
            _ => panic!("Unexpected update type"),
        }
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_market_data_to_trading_engine_flow() {
    // Create channels
    let (market_data_tx, market_data_rx) = market_data_channel();
    
    // Start market data engine (mock)
    // Start trading engine with receiver
    
    // Send test orderbook
    let orderbook = create_test_orderbook();
    market_data_tx.send(MarketDataUpdate::Orderbook(orderbook)).unwrap();
    
    // Verify trading engine received and processed
    // ...
}
```

## Success Criteria

- [ ] Channel types defined in `core_logic`
- [ ] Market Data Engine publishes via channels (not Redis)
- [ ] Trading Engine receives via channels (not Redis)
- [ ] Runner creates channel pairs and wires services
- [ ] All tests pass
- [ ] Performance: Channel latency < 1μs
- [ ] Redis code kept behind feature flag (optional)
- [ ] Documentation updated

## Rollback Plan

If issues arise during migration:

1. **Keep Redis code** behind feature flag
2. **Add configuration** to switch between channels and Redis
3. **Gradual migration**: Migrate one service at a time
4. **Feature toggle**: Allow runtime switching between implementations

## Future Considerations

### Web UI Support

If Web UI is implemented in the future:
- Redis can be used for Web UI → Monitoring Service communication
- Internal communication remains channel-based
- Redis becomes optional dependency (feature flag)

### External Services

If services are split into separate processes:
- Add gRPC layer on top of channels
- Channels remain for in-process communication
- gRPC used for inter-process communication

## Notes

- **Channels are for internal monolith communication only**
- **gRPC is for external services** (TUI CLI, future Web UI backend)
- **Redis is optional** and only for future Web UI (if needed)
- **Ultra-low latency** is critical for trading systems
- **No serialization overhead** with channels (direct memory access)
- **Simpler deployment** (one less service to manage)

