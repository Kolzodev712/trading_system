# Proposal 10: Testing Strategy

## Overview

This proposal outlines a comprehensive testing strategy for the trading system, covering unit tests, integration tests, and end-to-end system tests. Each service will have a complete set of unit tests and unique integration tests that validate exchange-specific functionality, market data handling, trading logic, and component interactions.

## Key Requirements

1. **Unit Tests**: Complete test coverage for each service's core logic
2. **Integration Tests**: Exchange-specific and service-specific integration tests
3. **Generalized Test Framework**: Reusable test infrastructure for exchange and market data engines
4. **Trading Engine Tests**: Comprehensive mocks for testing trading logic in realistic environments
5. **System Integration Tests**: End-to-end tests validating component interactions
6. **Debug Binary**: Trading engine binary for running the entire service without trading

## Testing Structure

### Service-Level Testing

Each service (`exchange_engine`, `market_data_engine`, `trading_engine`, `indicator_service`) will have:

1. **Unit Tests**: Test individual functions, methods, and modules in isolation
2. **Integration Tests**: Test service interactions with external dependencies (exchanges, databases)
3. **Mock Tests**: Test error scenarios and edge cases with mocked dependencies

### Exchange Engine Testing

**Purpose**: Validate exchange-specific functionality (REST API, WebSocket, trading operations).

**Test Framework**: Generalized test infrastructure that can be extended for each exchange.

#### Test Structure

```
exchange_engine/
├── src/
│   └── ... (implementation)
├── tests/
│   ├── common/                     # Shared test utilities
│   │   ├── mod.rs
│   │   ├── exchange_test_framework.rs  # Generalized test framework
│   │   ├── mock_exchange.rs       # Mock exchange server
│   │   ├── test_data_builder.rs    # Test data generators
│   │   └── assertions.rs           # Custom assertions
│   │
│   ├── binance/                    # Binance-specific tests
│   │   ├── mod.rs
│   │   ├── rest_tests.rs           # REST API tests
│   │   ├── websocket_tests.rs      # WebSocket tests
│   │   ├── trading_tests.rs        # Trading operation tests
│   │   └── integration_tests.rs    # End-to-end integration tests
│   │
│   └── kraken/                     # Kraken-specific tests (future)
│       └── ... (similar structure)
```

#### Generalized Test Framework

```rust
// exchange_engine/tests/common/exchange_test_framework.rs
pub trait ExchangeTestFramework {
    type ExchangeClient;
    type MockServer;
    
    /// Create a mock exchange server
    fn create_mock_server(&self) -> Self::MockServer;
    
    /// Create an exchange client for testing
    fn create_test_client(&self, server: &Self::MockServer) -> Self::ExchangeClient;
    
    /// Test REST API fetching
    fn test_rest_fetch<T>(&self, endpoint: &str, expected: T) -> Result<(), TestError>;
    
    /// Test WebSocket connection
    fn test_websocket_connection(&self) -> Result<(), TestError>;
    
    /// Test WebSocket message handling
    fn test_websocket_messages(&self, messages: Vec<WebSocketMessage>) -> Result<(), TestError>;
    
    /// Test order placement (open order)
    fn test_open_order(&self, order: OrderRequest) -> Result<(), TestError>;
    
    /// Test order cancellation by ID
    fn test_cancel_by_id(&self, order_id: &str) -> Result<(), TestError>;
    
    /// Test cancel all orders
    fn test_cancel_all(&self) -> Result<(), TestError>;
    
    /// Test order amendment (if available)
    fn test_amend_order(&self, order_id: &str, amendments: OrderAmendments) -> Result<(), TestError>;
    
    /// Test bulk order execution (if available)
    fn test_bulk_execute(&self, orders: Vec<OrderRequest>) -> Result<(), TestError>;
}
```

#### Binance Integration Tests

```rust
// exchange_engine/tests/binance/integration_tests.rs
#[tokio::test]
async fn test_binance_rest_fetch_balances() {
    let framework = BinanceTestFramework::new();
    let mock_server = framework.create_mock_server();
    let client = framework.create_test_client(&mock_server);
    
    let balances = client.fetch_balances().await.unwrap();
    assert_eq!(balances.len(), 2);
    assert_eq!(balances[0].symbol, "BTC");
}

#[tokio::test]
async fn test_binance_websocket_connection() {
    let framework = BinanceTestFramework::new();
    let mock_server = framework.create_mock_server();
    let client = framework.create_test_client(&mock_server);
    
    framework.test_websocket_connection().await.unwrap();
}

#[tokio::test]
async fn test_binance_open_order() {
    let framework = BinanceTestFramework::new();
    let mock_server = framework.create_mock_server();
    let client = framework.create_test_client(&mock_server);
    
    let order_request = OrderRequest {
        symbol: "BTCUSDT".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        quantity: Decimal::new(1, 0),
        price: Some(Decimal::new(50000, 0)),
    };
    
    framework.test_open_order(order_request).await.unwrap();
}

#[tokio::test]
async fn test_binance_cancel_by_id() {
    let framework = BinanceTestFramework::new();
    // ... test implementation
}

#[tokio::test]
async fn test_binance_cancel_all() {
    let framework = BinanceTestFramework::new();
    // ... test implementation
}
```

### Market Data Engine Testing

**Purpose**: Validate market data fetching (orderbooks, trades, snapshots) for each exchange.

**Test Framework**: Generalized test infrastructure similar to exchange engine.

#### Test Structure

```
market_data_engine/
├── src/
│   └── ... (implementation)
├── tests/
│   ├── common/                     # Shared test utilities
│   │   ├── mod.rs
│   │   ├── market_data_test_framework.rs  # Generalized test framework
│   │   ├── mock_market_data.rs     # Mock market data server
│   │   └── test_data_builder.rs    # Test data generators
│   │
│   ├── binance/                    # Binance-specific tests
│   │   ├── mod.rs
│   │   ├── orderbook_tests.rs      # Orderbook fetching tests
│   │   ├── trade_tests.rs           # Trade data tests
│   │   ├── snapshot_tests.rs        # Snapshot tests
│   │   └── integration_tests.rs     # End-to-end integration tests
│   │
│   └── kraken/                     # Kraken-specific tests (future)
│       └── ... (similar structure)
```

#### Generalized Test Framework

```rust
// market_data_engine/tests/common/market_data_test_framework.rs
pub trait MarketDataTestFramework {
    type MarketDataClient;
    type MockServer;
    
    /// Create a mock market data server
    fn create_mock_server(&self) -> Self::MockServer;
    
    /// Create a market data client for testing
    fn create_test_client(&self, server: &Self::MockServer) -> Self::MarketDataClient;
    
    /// Test orderbook fetching
    fn test_fetch_orderbook(&self, pair: &str) -> Result<(), TestError>;
    
    /// Test trade data fetching
    fn test_fetch_trades(&self, pair: &str) -> Result<(), TestError>;
    
    /// Test snapshot fetching
    fn test_fetch_snapshot(&self, pair: &str) -> Result<(), TestError>;
    
    /// Test WebSocket orderbook updates
    fn test_websocket_orderbook(&self, pair: &str) -> Result<(), TestError>;
    
    /// Test WebSocket trade updates
    fn test_websocket_trades(&self, pair: &str) -> Result<(), TestError>;
}
```

### Trading Engine Testing

**Purpose**: Test trading logic, level management, risk checks, and strategy execution.

**Test Approach**: Comprehensive mocks that simulate realistic trading environments.

#### Test Structure

```
trading_engine/
├── src/
│   └── ... (implementation)
├── tests/
│   ├── unit/                       # Unit tests
│   │   ├── level/
│   │   │   ├── manager_tests.rs
│   │   │   ├── executor_tests.rs
│   │   │   └── resolver_tests.rs
│   │   ├── risk/
│   │   │   ├── manager_tests.rs
│   │   │   ├── position_checks_tests.rs
│   │   │   └── balance_checks_tests.rs
│   │   └── strategy/
│   │       └── ... (strategy-specific tests)
│   │
│   ├── integration/               # Integration tests
│   │   ├── mod.rs
│   │   ├── mock_exchange.rs       # Mock exchange for trading tests
│   │   ├── mock_market_data.rs    # Mock market data for trading tests
│   │   ├── mock_indicator_service.rs # Mock indicator service
│   │   ├── level_integration_tests.rs
│   │   ├── risk_integration_tests.rs
│   │   └── trading_flow_tests.rs   # End-to-end trading flow
│   │
│   └── fixtures/                  # Test fixtures
│       ├── level_configs.json
│       ├── market_data.json
│       └── exchange_responses.json
```

#### Mock Trading Environment

```rust
// trading_engine/tests/integration/mock_exchange.rs
pub struct MockExchange {
    orders: Arc<RwLock<HashMap<String, Order>>>,
    balances: Arc<RwLock<HashMap<String, Balance>>>,
    positions: Arc<RwLock<HashMap<String, Position>>>,
    execution_reports: mpsc::UnboundedSender<ExecutionReport>,
}

impl MockExchange {
    pub fn new() -> Self {
        // Create mock exchange with realistic behavior
    }
    
    pub async fn place_order(&self, order: OrderRequest) -> Result<Order, ExchangeError> {
        // Simulate order placement
        // Generate execution reports
        // Update balances/positions
    }
    
    pub async fn cancel_order(&self, order_id: &str) -> Result<(), ExchangeError> {
        // Simulate order cancellation
    }
    
    pub fn get_execution_reports(&self) -> mpsc::UnboundedReceiver<ExecutionReport> {
        // Return receiver for execution reports
    }
}
```

#### Trading Flow Integration Tests

```rust
// trading_engine/tests/integration/trading_flow_tests.rs
#[tokio::test]
async fn test_level_execution_flow() {
    // 1. Setup mocks
    let mock_exchange = MockExchange::new();
    let mock_market_data = MockMarketData::new();
    let mock_indicators = MockIndicatorService::new();
    
    // 2. Create trading engine with mocks
    let trading_engine = TradingEngine::new_with_mocks(
        mock_exchange,
        mock_market_data,
        mock_indicators,
    );
    
    // 3. Configure a level
    let level_config = LevelConfig {
        id: "test_level_1".to_string(),
        pair: "BTCUSDT".to_string(),
        side: OrderSide::Buy,
        price: Some(Decimal::new(50000, 0)),
        quantity: Some(Decimal::new(1, 0)),
    };
    trading_engine.add_level(level_config).await.unwrap();
    
    // 4. Simulate market data update
    mock_market_data.send_orderbook_update(/* ... */).await;
    
    // 5. Verify order was placed
    let orders = mock_exchange.get_orders().await;
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].side, OrderSide::Buy);
    
    // 6. Simulate order fill
    mock_exchange.simulate_fill(&orders[0].id).await;
    
    // 7. Verify position was updated
    let positions = trading_engine.get_positions().await;
    assert_eq!(positions.len(), 1);
}
```

### Indicator Service Testing

**Purpose**: Test indicator calculations and value publishing.

#### Test Structure

```
indicator_service/
├── src/
│   └── ... (implementation)
├── tests/
│   ├── unit/
│   │   ├── sma_tests.rs
│   │   ├── ema_tests.rs
│   │   └── ... (other indicator tests)
│   │
│   └── integration/
│       ├── mod.rs
│       ├── calculation_tests.rs
│       └── publishing_tests.rs
```

### System Integration Tests

**Purpose**: Test interactions between all components in the trading system.

#### Test Structure

```
trading_system/
├── tests/
│   ├── integration/
│   │   ├── mod.rs
│   │   ├── component_interaction_tests.rs
│   │   ├── channel_communication_tests.rs
│   │   ├── grpc_communication_tests.rs
│   │   └── end_to_end_tests.rs
│   │
│   └── fixtures/
│       └── ... (shared test fixtures)
```

#### Component Interaction Tests

```rust
// tests/integration/component_interaction_tests.rs
#[tokio::test]
async fn test_exchange_to_trading_engine_flow() {
    // 1. Setup all components
    let (exchange_tx, exchange_rx) = mpsc::unbounded_channel();
    let (market_data_tx, market_data_rx) = mpsc::unbounded_channel();
    
    let exchange_engine = ExchangeEngine::new(exchange_tx);
    let market_data_engine = MarketDataEngine::new(market_data_tx);
    let trading_engine = TradingEngine::new(exchange_rx, market_data_rx);
    
    // 2. Start all components
    tokio::spawn(async move { exchange_engine.run().await });
    tokio::spawn(async move { market_data_engine.run().await });
    tokio::spawn(async move { trading_engine.run().await });
    
    // 3. Simulate exchange update
    exchange_engine.simulate_order_update(/* ... */).await;
    
    // 4. Verify trading engine received update
    // (via assertions or test hooks)
}
```

### Debug Binary

**Purpose**: Run the entire trading system without placing real orders for debugging and validation.

#### Implementation

```
trading_engine/
├── src/
│   └── bin/
│       └── debug.rs                # Debug binary
```

```rust
// trading_engine/src/bin/debug.rs
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load configuration
    let config = load_config("config/trading_system.json")?;
    
    // 2. Initialize all components with MOCK exchange
    let mock_exchange = MockExchange::new();
    let exchange_engine = ExchangeEngine::new_with_mock(mock_exchange);
    let market_data_engine = MarketDataEngine::new(/* ... */);
    let trading_engine = TradingEngine::new(/* ... */);
    
    // 3. Start all components
    // 4. Run event loop
    // 5. Log all operations (no real trading)
    
    Ok(())
}
```

**Usage**:
```bash
cargo run --bin trading-engine-debug -- --config config/trading_system.json
```

## Test Execution Strategy

### Unit Tests
- Run on every commit
- Fast execution (< 1 second per test)
- No external dependencies

### Integration Tests
- Run on pull requests and before releases
- May require mock servers or test exchanges
- Moderate execution time (< 10 seconds per test)

### System Integration Tests
- Run before releases
- May require full system setup
- Longer execution time (< 1 minute per test)

## Test Coverage Goals

- **Unit Tests**: 80%+ code coverage
- **Integration Tests**: All critical paths covered
- **System Tests**: All component interactions validated

## Continuous Integration

Tests should be integrated into CI/CD pipeline:
1. Run unit tests on every commit
2. Run integration tests on pull requests
3. Run system tests before releases
4. Generate coverage reports

## Success Criteria

- [ ] All services have comprehensive unit tests
- [ ] Exchange engine has generalized test framework
- [ ] Market data engine has generalized test framework
- [ ] Trading engine has mock-based integration tests
- [ ] System integration tests validate component interactions
- [ ] Debug binary allows running system without trading
- [ ] Test coverage meets goals (80%+ for unit tests)
- [ ] All tests pass in CI/CD pipeline

