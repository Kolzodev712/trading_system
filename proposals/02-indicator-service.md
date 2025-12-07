# Proposal 02: Indicator Service

## Overview

This proposal outlines the implementation of an Indicator Service that calculates and provides technical indicator values (SMA, EMA, RSI, etc.) to the trading system. The Indicator Service is a separate crate within the same workspace, communicating via in-process channels for ultra-low latency. It subscribes to market data updates, calculates indicators in real-time, and publishes indicator values to subscribers (primarily the Trading Engine).

**Key Insight**: The Indicator Service is a **prerequisite** for Proposal 03 (Level-Based Trading System), as levels depend on indicator values for dynamic order requirements.

## Key Requirements

1. **In-Process Service**: Separate crate but same monolith, uses channels for communication
2. **Real-Time Calculation**: Calculates indicators from live market data (orderbooks, trades)
3. **Channel-Based Communication**: Publishes indicator values via channels (not gRPC)
4. **Caching**: Caches calculated values to avoid redundant calculations
5. **Extensible**: Easy to add new indicators
6. **Performance**: Ultra-low latency (< 0.1ms for indicator lookup)
7. **Initial Indicators**: SMA (Simple Moving Average) and EMA (Exponential Moving Average)

## Architecture Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│              Trading System (Monolith)                   │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Market Data Engine                              │   │
│  │  - Receives orderbook/trade updates from WS     │   │
│  └──────────────┬───────────────────────────────────┘   │
│                 │                                        │
│                 │ Channel (MarketDataUpdate)            │
│                 ▼                                        │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Indicator Service (Separate Crate)              │   │
│  │  - Subscribes to market data                     │   │
│  │  - Calculates indicators (SMA, EMA, etc.)        │   │
│  │  - Caches indicator values                        │   │
│  │  - Publishes indicator values                     │   │
│  └──────────────┬───────────────────────────────────┘   │
│                 │                                        │
│                 │ Channel (IndicatorValue)               │
│                 ▼                                        │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Trading Engine                                  │   │
│  │  - Subscribes to indicator values                │   │
│  │  - Uses indicators for level resolution          │   │
│  └──────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

### Data Flow

```
1. Market Data Update:
   Market Data Engine → Channel (MarketDataUpdate) → Indicator Service

2. Indicator Calculation:
   Indicator Service → Calculate indicator → Cache value → Publish to channel

3. Indicator Consumption:
   Indicator Service → Channel (IndicatorValue) → Trading Engine
```

## Component Structure

```
trading_system/
├── src/
│   ├── indicator_service/          # NEW: Indicator Service crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs              # Service entry point (if run separately)
│   │   │   ├── service.rs           # Main service orchestrator
│   │   │   ├── calculator.rs        # Indicator calculator coordinator
│   │   │   ├── cache.rs             # Indicator value cache
│   │   │   ├── publisher.rs         # Publishes indicator values via channels
│   │   │   ├── subscriber.rs         # Subscribes to market data
│   │   │   │
│   │   │   ├── indicators/          # Indicator implementations
│   │   │   │   ├── mod.rs
│   │   │   │   ├── traits.rs         # Indicator trait
│   │   │   │   ├── sma.rs            # Simple Moving Average
│   │   │   │   ├── ema.rs            # Exponential Moving Average
│   │   │   │   ├── rsi.rs            # Relative Strength Index (future)
│   │   │   │   ├── macd.rs            # MACD (future)
│   │   │   │   └── bollinger.rs      # Bollinger Bands (future)
│   │   │   │
│   │   │   ├── data/                 # Data structures
│   │   │   │   ├── mod.rs
│   │   │   │   ├── price_point.rs    # Price point for calculations
│   │   │   │   └── indicator_value.rs # Indicator value type
│   │   │   │
│   │   │   └── config.rs             # Indicator service configuration
│   │   │
│   │   └── tests/
│   │       ├── unit/
│   │       │   ├── sma_tests.rs
│   │       │   ├── ema_tests.rs
│   │       │   └── calculator_tests.rs
│   │       └── integration/
│   │           └── service_tests.rs
```

## Detailed Component Design

### 1. Indicator Trait (`indicator_service/src/indicators/traits.rs`)

**Purpose**: Defines the interface that all indicators must implement.

```rust
use rust_decimal::Decimal;
use std::collections::VecDeque;
use crate::data::price_point::PricePoint;

/// Trait that all indicators must implement
pub trait Indicator: Send + Sync {
    /// Unique identifier for this indicator instance
    fn id(&self) -> &str;
    
    /// Indicator name (e.g., "SMA", "EMA")
    fn name(&self) -> &str;
    
    /// Trading pair this indicator is for
    fn pair(&self) -> &str;
    
    /// Period/length of the indicator (e.g., 20 for SMA20)
    fn period(&self) -> usize;
    
    /// Update the indicator with a new price point
    fn update(&mut self, price_point: PricePoint) -> Result<(), IndicatorError>;
    
    /// Get the current indicator value
    fn value(&self) -> Option<Decimal>;
    
    /// Check if the indicator has enough data to calculate a value
    fn is_ready(&self) -> bool;
    
    /// Reset the indicator (clear all data)
    fn reset(&mut self);
    
    /// Get the indicator configuration
    fn config(&self) -> IndicatorConfig;
}

/// Indicator configuration
#[derive(Debug, Clone)]
pub struct IndicatorConfig {
    pub id: String,
    pub name: String,
    pub pair: String,
    pub period: usize,
    pub source: PriceSource,  // Close, Open, High, Low, Volume, etc.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    Close,
    Open,
    High,
    Low,
    Volume,
    TypicalPrice,  // (High + Low + Close) / 3
    WeightedPrice, // (High + Low + 2*Close) / 4
}

#[derive(Debug, thiserror::Error)]
pub enum IndicatorError {
    #[error("Insufficient data: need {needed} points, have {have}")]
    InsufficientData { needed: usize, have: usize },
    
    #[error("Invalid period: {period}")]
    InvalidPeriod { period: usize },
    
    #[error("Calculation error: {0}")]
    CalculationError(String),
}
```

### 2. Simple Moving Average (`indicator_service/src/indicators/sma.rs`)

**Purpose**: Implements Simple Moving Average (SMA) indicator.

```rust
use rust_decimal::Decimal;
use std::collections::VecDeque;
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError, PriceSource};
use crate::data::price_point::PricePoint;

/// Simple Moving Average (SMA) indicator
pub struct SimpleMovingAverage {
    config: IndicatorConfig,
    prices: VecDeque<Decimal>,
    current_sum: Decimal,
}

impl SimpleMovingAverage {
    /// Create a new SMA indicator
    pub fn new(config: IndicatorConfig) -> Result<Self, IndicatorError> {
        if config.period == 0 {
            return Err(IndicatorError::InvalidPeriod { period: config.period });
        }
        
        Ok(Self {
            config,
            prices: VecDeque::with_capacity(config.period),
            current_sum: Decimal::ZERO,
        })
    }
    
    /// Get the price value based on the configured source
    fn get_price_value(&self, price_point: &PricePoint) -> Decimal {
        match self.config.source {
            PriceSource::Close => price_point.close,
            PriceSource::Open => price_point.open,
            PriceSource::High => price_point.high,
            PriceSource::Low => price_point.low,
            PriceSource::Volume => price_point.volume,
            PriceSource::TypicalPrice => {
                (price_point.high + price_point.low + price_point.close) / Decimal::from(3)
            }
            PriceSource::WeightedPrice => {
                (price_point.high + price_point.low + Decimal::from(2) * price_point.close) / Decimal::from(4)
            }
        }
    }
}

impl Indicator for SimpleMovingAverage {
    fn id(&self) -> &str {
        &self.config.id
    }
    
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn pair(&self) -> &str {
        &self.config.pair
    }
    
    fn period(&self) -> usize {
        self.config.period
    }
    
    fn update(&mut self, price_point: PricePoint) -> Result<(), IndicatorError> {
        let price = self.get_price_value(&price_point);
        
        // If we have reached the period, remove the oldest value
        if self.prices.len() >= self.config.period {
            if let Some(old_price) = self.prices.pop_front() {
                self.current_sum -= old_price;
            }
        }
        
        // Add the new price
        self.prices.push_back(price);
        self.current_sum += price;
        
        Ok(())
    }
    
    fn value(&self) -> Option<Decimal> {
        if !self.is_ready() {
            return None;
        }
        
        Some(self.current_sum / Decimal::from(self.prices.len()))
    }
    
    fn is_ready(&self) -> bool {
        self.prices.len() >= self.config.period
    }
    
    fn reset(&mut self) {
        self.prices.clear();
        self.current_sum = Decimal::ZERO;
    }
    
    fn config(&self) -> IndicatorConfig {
        self.config.clone()
    }
}
```

### 3. Exponential Moving Average (`indicator_service/src/indicators/ema.rs`)

**Purpose**: Implements Exponential Moving Average (EMA) indicator.

```rust
use rust_decimal::Decimal;
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError, PriceSource};
use crate::data::price_point::PricePoint;

/// Exponential Moving Average (EMA) indicator
pub struct ExponentialMovingAverage {
    config: IndicatorConfig,
    current_ema: Option<Decimal>,
    multiplier: Decimal,  // 2 / (period + 1)
    price_count: usize,
}

impl ExponentialMovingAverage {
    /// Create a new EMA indicator
    pub fn new(config: IndicatorConfig) -> Result<Self, IndicatorError> {
        if config.period == 0 {
            return Err(IndicatorError::InvalidPeriod { period: config.period });
        }
        
        // Calculate multiplier: 2 / (period + 1)
        let multiplier = Decimal::from(2) / Decimal::from(config.period + 1);
        
        Ok(Self {
            config,
            current_ema: None,
            multiplier,
            price_count: 0,
        })
    }
    
    /// Get the price value based on the configured source
    fn get_price_value(&self, price_point: &PricePoint) -> Decimal {
        match self.config.source {
            PriceSource::Close => price_point.close,
            PriceSource::Open => price_point.open,
            PriceSource::High => price_point.high,
            PriceSource::Low => price_point.low,
            PriceSource::Volume => price_point.volume,
            PriceSource::TypicalPrice => {
                (price_point.high + price_point.low + price_point.close) / Decimal::from(3)
            }
            PriceSource::WeightedPrice => {
                (price_point.high + price_point.low + Decimal::from(2) * price_point.close) / Decimal::from(4)
            }
        }
    }
}

impl Indicator for ExponentialMovingAverage {
    fn id(&self) -> &str {
        &self.config.id
    }
    
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn pair(&self) -> &str {
        &self.config.pair
    }
    
    fn period(&self) -> usize {
        self.config.period
    }
    
    fn update(&mut self, price_point: PricePoint) -> Result<(), IndicatorError> {
        let price = self.get_price_value(&price_point);
        self.price_count += 1;
        
        match self.current_ema {
            None => {
                // First value: use the price directly
                self.current_ema = Some(price);
            }
            Some(prev_ema) => {
                // EMA = (Price - Previous EMA) * Multiplier + Previous EMA
                // Simplified: EMA = Price * Multiplier + Previous EMA * (1 - Multiplier)
                let new_ema = price * self.multiplier + prev_ema * (Decimal::ONE - self.multiplier);
                self.current_ema = Some(new_ema);
            }
        }
        
        Ok(())
    }
    
    fn value(&self) -> Option<Decimal> {
        self.current_ema
    }
    
    fn is_ready(&self) -> bool {
        self.price_count >= self.config.period
    }
    
    fn reset(&mut self) {
        self.current_ema = None;
        self.price_count = 0;
    }
    
    fn config(&self) -> IndicatorConfig {
        self.config.clone()
    }
}
```

### 4. Price Point (`indicator_service/src/data/price_point.rs`)

**Purpose**: Represents a single price point (from orderbook or trade).

```rust
use rust_decimal::Decimal;
use std::time::{SystemTime, UNIX_EPOCH};

/// A price point used for indicator calculations
#[derive(Debug, Clone, Copy)]
pub struct PricePoint {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: u64,
}

impl PricePoint {
    /// Create a price point from a trade
    pub fn from_trade(price: Decimal, quantity: Decimal, timestamp: u64) -> Self {
        Self {
            open: price,
            high: price,
            low: price,
            close: price,
            volume: quantity,
            timestamp,
        }
    }
    
    /// Create a price point from an orderbook snapshot
    pub fn from_orderbook(
        bid_price: Decimal,
        ask_price: Decimal,
        volume: Decimal,
    ) -> Self {
        let mid_price = (bid_price + ask_price) / Decimal::from(2);
        Self {
            open: mid_price,
            high: ask_price,
            low: bid_price,
            close: mid_price,
            volume,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
    
    /// Create a price point from OHLCV data
    pub fn from_ohlcv(
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Decimal,
        timestamp: u64,
    ) -> Self {
        Self {
            open,
            high,
            low,
            close,
            volume,
            timestamp,
        }
    }
}
```

### 5. Indicator Value (`indicator_service/src/data/indicator_value.rs`)

**Purpose**: Represents a calculated indicator value.

```rust
use rust_decimal::Decimal;

/// An indicator value with metadata
#[derive(Debug, Clone)]
pub struct IndicatorValue {
    /// Indicator instance ID
    pub indicator_id: String,
    
    /// Indicator name (e.g., "SMA", "EMA")
    pub indicator_name: String,
    
    /// Trading pair
    pub pair: String,
    
    /// Calculated value
    pub value: Decimal,
    
    /// Timestamp when the value was calculated
    pub timestamp: u64,
    
    /// Period/length of the indicator
    pub period: usize,
}
```

### 6. Indicator Calculator (`indicator_service/src/calculator.rs`)

**Purpose**: Coordinates indicator calculations and manages indicator instances.

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use crate::indicators::traits::{Indicator, IndicatorConfig, IndicatorError};
use crate::indicators::{SimpleMovingAverage, ExponentialMovingAverage};
use crate::data::{PricePoint, IndicatorValue};

pub struct IndicatorCalculator {
    indicators: Arc<RwLock<HashMap<String, Box<dyn Indicator + Send + Sync>>>>,
    cache: Arc<RwLock<HashMap<String, IndicatorValue>>>,  // indicator_id -> latest value
}

impl IndicatorCalculator {
    pub fn new() -> Self {
        Self {
            indicators: Arc::new(RwLock::new(HashMap::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register a new indicator
    pub fn register_indicator(
        &self,
        config: IndicatorConfig,
    ) -> Result<(), IndicatorError> {
        let mut indicators = self.indicators.write().unwrap();
        
        let indicator: Box<dyn Indicator + Send + Sync> = match config.name.as_str() {
            "SMA" => Box::new(SimpleMovingAverage::new(config.clone())?),
            "EMA" => Box::new(ExponentialMovingAverage::new(config.clone())?),
            _ => return Err(IndicatorError::CalculationError(
                format!("Unknown indicator: {}", config.name)
            )),
        };
        
        indicators.insert(config.id.clone(), indicator);
        Ok(())
    }
    
    /// Update all indicators with a new price point
    pub fn update(&self, pair: &str, price_point: PricePoint) -> Result<Vec<IndicatorValue>, IndicatorError> {
        let mut updated_values = Vec::new();
        
        // Get all indicators for this pair
        let indicators = self.indicators.read().unwrap();
        let pair_indicators: Vec<_> = indicators
            .values()
            .filter(|ind| ind.pair() == pair)
            .collect();
        
        // Update each indicator
        for indicator in pair_indicators {
            let indicator_id = indicator.id().to_string();
            let mut indicator_mut = indicators.get_mut(&indicator_id)
                .ok_or_else(|| IndicatorError::CalculationError("Indicator not found".to_string()))?;
            
            indicator_mut.update(price_point)?;
            
            // If indicator is ready, get its value and cache it
            if indicator_mut.is_ready() {
                if let Some(value) = indicator_mut.value() {
                    let indicator_value = IndicatorValue {
                        indicator_id: indicator_id.clone(),
                        indicator_name: indicator_mut.name().to_string(),
                        pair: pair.to_string(),
                        value,
                        timestamp: price_point.timestamp,
                        period: indicator_mut.period(),
                    };
                    
                    // Update cache
                    let mut cache = self.cache.write().unwrap();
                    cache.insert(indicator_id.clone(), indicator_value.clone());
                    
                    updated_values.push(indicator_value);
                }
            }
        }
        
        Ok(updated_values)
    }
    
    /// Get cached indicator value
    pub fn get_value(&self, indicator_id: &str) -> Option<IndicatorValue> {
        let cache = self.cache.read().unwrap();
        cache.get(indicator_id).cloned()
    }
    
    /// Get all cached values for a pair
    pub fn get_values_for_pair(&self, pair: &str) -> Vec<IndicatorValue> {
        let cache = self.cache.read().unwrap();
        cache
            .values()
            .filter(|v| v.pair == pair)
            .cloned()
            .collect()
    }
    
    /// Remove an indicator
    pub fn remove_indicator(&self, indicator_id: &str) {
        let mut indicators = self.indicators.write().unwrap();
        indicators.remove(indicator_id);
        
        let mut cache = self.cache.write().unwrap();
        cache.remove(indicator_id);
    }
}
```

### 7. Market Data Subscriber (`indicator_service/src/subscriber.rs`)

**Purpose**: Subscribes to market data updates from Market Data Engine.

```rust
use tokio::sync::mpsc;
use core_logic::channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver};
use crate::calculator::IndicatorCalculator;
use crate::data::PricePoint;

pub struct MarketDataSubscriber {
    receiver: MarketDataUpdateReceiver,
    calculator: Arc<IndicatorCalculator>,
}

impl MarketDataSubscriber {
    pub fn new(
        receiver: MarketDataUpdateReceiver,
        calculator: Arc<IndicatorCalculator>,
    ) -> Self {
        Self {
            receiver,
            calculator,
        }
    }
    
    /// Start listening for market data updates
    pub async fn run(&mut self) -> Result<(), SubscriberError> {
        while let Some(update) = self.receiver.recv().await {
            self.handle_update(update).await?;
        }
        Ok(())
    }
    
    async fn handle_update(&self, update: MarketDataUpdate) -> Result<(), SubscriberError> {
        // Process orderbook updates
        for orderbook in update.orderbooks {
            let price_point = PricePoint::from_orderbook(
                orderbook.best_bid_price(),
                orderbook.best_ask_price(),
                orderbook.total_volume(),
            );
            
            self.calculator.update(&orderbook.pair.to_string(), price_point)?;
        }
        
        // Process trade updates
        for trade in update.trades {
            let price_point = PricePoint::from_trade(
                trade.price,
                trade.quantity,
                trade.timestamp,
            );
            
            self.calculator.update(&trade.pair.to_string(), price_point)?;
        }
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubscriberError {
    #[error("Channel error: {0}")]
    ChannelError(String),
    
    #[error("Calculator error: {0}")]
    CalculatorError(#[from] crate::indicators::traits::IndicatorError),
}
```

### 8. Indicator Publisher (`indicator_service/src/publisher.rs`)

**Purpose**: Publishes indicator values to subscribers via channels.

```rust
use tokio::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use crate::calculator::IndicatorCalculator;
use crate::data::IndicatorValue;
use core_logic::channels::indicators::{IndicatorUpdateSender, IndicatorUpdate};

pub struct IndicatorPublisher {
    calculator: Arc<IndicatorCalculator>,
    sender: IndicatorUpdateSender,
    publish_interval: tokio::time::Duration,
}

impl IndicatorPublisher {
    pub fn new(
        calculator: Arc<IndicatorCalculator>,
        sender: IndicatorUpdateSender,
        publish_interval_secs: u64,
    ) -> Self {
        Self {
            calculator,
            sender,
            publish_interval: tokio::time::Duration::from_secs(publish_interval_secs),
        }
    }
    
    /// Start publishing indicator values periodically
    pub async fn run(&self) -> Result<(), PublisherError> {
        let mut interval = tokio::time::interval(self.publish_interval);
        
        loop {
            interval.tick().await;
            
            // Get all cached values
            let cache = self.calculator.cache.read().unwrap();
            let values: Vec<IndicatorValue> = cache.values().cloned().collect();
            drop(cache);
            
            // Publish each value
            for value in values {
                let update = IndicatorUpdate {
                    indicator_id: value.indicator_id.clone(),
                    indicator_name: value.indicator_name.clone(),
                    pair: value.pair.clone(),
                    value: value.value,
                    timestamp: value.timestamp,
                    period: value.period,
                };
                
                if let Err(e) = self.sender.send(update) {
                    return Err(PublisherError::ChannelError(format!("Failed to send: {}", e)));
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublisherError {
    #[error("Channel error: {0}")]
    ChannelError(String),
}
```

### 9. Indicator Service (`indicator_service/src/service.rs`)

**Purpose**: Main orchestrator for the Indicator Service.

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use core_logic::channels::market_data::{MarketDataUpdateReceiver};
use core_logic::channels::indicators::{IndicatorUpdateSender};
use crate::calculator::IndicatorCalculator;
use crate::subscriber::MarketDataSubscriber;
use crate::publisher::IndicatorPublisher;
use crate::indicators::traits::IndicatorConfig;

pub struct IndicatorService {
    calculator: Arc<IndicatorCalculator>,
    subscriber: Option<MarketDataSubscriber>,
    publisher: Option<IndicatorPublisher>,
}

impl IndicatorService {
    pub fn new() -> Self {
        Self {
            calculator: Arc::new(IndicatorCalculator::new()),
            subscriber: None,
            publisher: None,
        }
    }
    
    /// Initialize the service with channels
    pub fn initialize(
        &mut self,
        market_data_receiver: MarketDataUpdateReceiver,
        indicator_sender: IndicatorUpdateSender,
        publish_interval_secs: u64,
    ) {
        self.subscriber = Some(MarketDataSubscriber::new(
            market_data_receiver,
            self.calculator.clone(),
        ));
        
        self.publisher = Some(IndicatorPublisher::new(
            self.calculator.clone(),
            indicator_sender,
            publish_interval_secs,
        ));
    }
    
    /// Register an indicator
    pub fn register_indicator(
        &self,
        config: IndicatorConfig,
    ) -> Result<(), crate::indicators::traits::IndicatorError> {
        self.calculator.register_indicator(config)
    }
    
    /// Get indicator value
    pub fn get_value(&self, indicator_id: &str) -> Option<crate::data::IndicatorValue> {
        self.calculator.get_value(indicator_id)
    }
    
    /// Get all values for a pair
    pub fn get_values_for_pair(&self, pair: &str) -> Vec<crate::data::IndicatorValue> {
        self.calculator.get_values_for_pair(pair)
    }
    
    /// Start the service (run subscriber and publisher)
    pub async fn run(&mut self) -> Result<(), ServiceError> {
        let subscriber = self.subscriber.take()
            .ok_or(ServiceError::NotInitialized)?;
        let publisher = self.publisher.take()
            .ok_or(ServiceError::NotInitialized)?;
        
        // Run subscriber and publisher concurrently
        tokio::select! {
            result = subscriber.run() => {
                result.map_err(ServiceError::SubscriberError)
            }
            result = publisher.run() => {
                result.map_err(ServiceError::PublisherError)
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service not initialized")]
    NotInitialized,
    
    #[error("Subscriber error: {0}")]
    SubscriberError(#[from] crate::subscriber::SubscriberError),
    
    #[error("Publisher error: {0}")]
    PublisherError(#[from] crate::publisher::PublisherError),
}
```

### 10. Channel Types (`core_logic/src/channels/indicators.rs`)

**Purpose**: Defines channel types for indicator communication (to be added to `core_logic`).

```rust
use tokio::sync::mpsc;
use rust_decimal::Decimal;

pub type IndicatorUpdateSender = mpsc::UnboundedSender<IndicatorUpdate>;
pub type IndicatorUpdateReceiver = mpsc::UnboundedReceiver<IndicatorUpdate>;

#[derive(Debug, Clone)]
pub struct IndicatorUpdate {
    pub indicator_id: String,
    pub indicator_name: String,
    pub pair: String,
    pub value: Decimal,
    pub timestamp: u64,
    pub period: usize,
}
```

## Integration in Runner

The `runner/src/main.rs` will be responsible for:

1. **Creating Channel Pairs**:
   ```rust
   let (market_data_tx, market_data_rx) = mpsc::unbounded_channel();
   let (indicator_tx, indicator_rx) = mpsc::unbounded_channel();
   ```

2. **Initializing Indicator Service**:
   ```rust
   let mut indicator_service = IndicatorService::new();
   indicator_service.initialize(
       market_data_rx,
       indicator_tx,
       1, // Publish every 1 second
   );
   
   // Register indicators
   indicator_service.register_indicator(IndicatorConfig {
       id: "sma_20_btc".to_string(),
       name: "SMA".to_string(),
       pair: "BTCUSDT".to_string(),
       period: 20,
       source: PriceSource::Close,
   })?;
   ```

3. **Starting Indicator Service**:
   ```rust
   tokio::spawn(async move {
       indicator_service.run().await.unwrap();
   });
   ```

4. **Passing Indicator Receiver to Trading Engine**:
   ```rust
   let trading_engine = TradingEngine::new(
       // ... other channels
       indicator_rx,
   );
   ```

## Configuration

### Indicator Service Configuration

```json
{
  "indicator_service": {
    "publish_interval_secs": 1,
    "indicators": [
      {
        "id": "sma_20_btc",
        "name": "SMA",
        "pair": "BTCUSDT",
        "period": 20,
        "source": "Close"
      },
      {
        "id": "ema_12_btc",
        "name": "EMA",
        "pair": "BTCUSDT",
        "period": 12,
        "source": "Close"
      }
    ]
  }
}
```

## Performance Considerations

1. **Caching**: Indicator values are cached to avoid redundant calculations
2. **Incremental Updates**: Indicators update incrementally (no full recalculation)
3. **Channel Communication**: Ultra-low latency (< 1μs) for in-process communication
4. **Efficient Data Structures**: Use `VecDeque` for price history (O(1) push/pop)
5. **Minimal Allocations**: Reuse buffers where possible

## Error Handling

1. **Insufficient Data**: Indicators return `None` until enough data points are collected
2. **Calculation Errors**: Errors are logged and processing continues
3. **Channel Errors**: Handle channel disconnections gracefully

## Testing Strategy

1. **Unit Tests**: Test each indicator calculation independently
2. **Integration Tests**: Test indicator service with mock market data
3. **Performance Tests**: Benchmark indicator calculations

## Success Criteria

- [ ] Indicator Service calculates SMA and EMA correctly
- [ ] Indicator values are published via channels
- [ ] Trading Engine can subscribe and receive indicator values
- [ ] Indicator values are cached for fast lookup
- [ ] Service handles market data updates in real-time
- [ ] Performance meets targets (< 0.1ms for lookup)

## Future Enhancements

- Additional indicators (RSI, MACD, Bollinger Bands, etc.)
- Multi-timeframe support
- Indicator combinations
- Custom indicator definitions

