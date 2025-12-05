// Integration tests for the trading system
// These tests verify that all components work together correctly

mod integration {
    use core_logic::{config::TradingSystemConfig, data::base_data::Pair, traits::TradingClient};
    use exchange_engine::exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use trading_engine::{strategies::BuyLowSellHigh, strategy::Strategy};

    /// Test that the system can load configuration
    #[tokio::test]
    async fn test_config_loading() {
        let config = TradingSystemConfig::load_from_file("config/trading_system.json");
        assert!(config.is_ok(), "Should load config file");

        let config = config.unwrap();
        assert_eq!(config.exchange.exchange, core_logic::Exchange::Binance);
        assert_eq!(config.strategy.strategy_type, "BuyLowSellHigh");
    }

    /// Test that configuration validation works
    #[tokio::test]
    async fn test_config_validation() {
        let config = TradingSystemConfig::load_from_file("config/trading_system.json").unwrap();
        let result = config.validate();
        assert!(result.is_ok(), "Config should be valid: {:?}", result.err());
    }

    /// Test that strategy can be created
    #[tokio::test]
    async fn test_strategy_creation() {
        // Create a mock trading client (we'll use a real one but won't connect)
        let config = TradingSystemConfig::load_from_file("config/trading_system.json").unwrap();
        let binance_config_json = config.exchange.exchange_specific.get("binance").unwrap();

        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        let trading_client: Arc<
            RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(binance_config, 1024)));

        let strategy = BuyLowSellHigh::new(
            trading_client,
            symbol,
            rust_decimal::Decimal::new(1, 3),        // 0.001
            rust_decimal::Decimal::new(1, 2),        // 0.01
            rust_decimal::Decimal::new(2, 0),        // 2.0
            rust_decimal::Decimal::new(2, 0),        // 2.0
            Some(rust_decimal::Decimal::new(5, 0)),  // 5.0
            Some(rust_decimal::Decimal::new(10, 0)), // 10.0
            2,
            100,
            20,
        );

        // Test that strategy has initial state
        let state = strategy.get_state();
        assert_eq!(state.positions.len(), 0);
        assert_eq!(state.active_orders.len(), 0);
    }

    /// Test order status tracking
    #[tokio::test]
    async fn test_order_status_tracking() {
        use core_logic::data::{
            base_data::{OrderId, OrderStatus},
            macros::Symbol,
            private_data::Order,
        };

        let mut state = trading_engine::strategy::TradingState::new();

        // Create a test order
        let order = Order {
            order_id: OrderId::new("test123".into()),
            symbol: Symbol::from("BTCUSDT"),
            side: core_logic::data::base_data::Side::Buy,
            price: rust_decimal::Decimal::new(50000, 0),
            amount: rust_decimal::Decimal::new(1, 3),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: rust_decimal::Decimal::ZERO,
        };

        // Add order
        state.add_order(order.clone());

        // Check it's tracked as pending
        assert_eq!(state.pending_orders.len(), 1);
        assert_eq!(state.active_orders.len(), 1);

        // Update to filled
        state.update_order_status(&order.order_id, OrderStatus::Filled, Some(order.amount));

        // Should be removed from pending
        assert_eq!(state.pending_orders.len(), 0);

        // Check order status was updated
        let updated_order = state.active_orders.get(&order.order_id).unwrap();
        assert_eq!(updated_order.status, OrderStatus::Filled);
        assert_eq!(updated_order.filled_amount, order.amount);
    }

    /// Test circuit breaker
    #[tokio::test]
    async fn test_circuit_breaker() {
        use core_logic::circuit_breaker::CircuitBreaker;

        let cb = CircuitBreaker::new(3, 60);

        // Initially should allow requests
        assert!(cb.can_proceed());

        // Record failures
        cb.record_failure();
        cb.record_failure();
        assert!(cb.can_proceed()); // Still under threshold

        cb.record_failure();
        // Should be open now
        assert!(!cb.can_proceed());

        // Record success (shouldn't help when open)
        cb.record_success();
        assert!(!cb.can_proceed());
    }

    /// Test position reconciliation logic (without actual exchange call)
    #[tokio::test]
    async fn test_position_reconciliation_logic() {
        use core_logic::data::base_data::Pair;
        use trading_engine::strategy::TradingState;

        let mut state = TradingState::new();
        let symbol = Pair::new("BTC", "USDT");

        // Add a position
        state.update_position(
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3), // 0.001 BTC
            rust_decimal::Decimal::new(50000, 0),
        );

        // Check position exists
        let position = state.get_position(&symbol);
        assert!(position.is_some());
        assert_eq!(position.unwrap().quantity, rust_decimal::Decimal::new(1, 3));
    }

    /// Test stale order detection
    #[tokio::test]
    async fn test_stale_order_detection() {
        use core_logic::data::{
            base_data::{OrderId, OrderStatus},
            macros::Symbol,
            private_data::Order,
        };
        use trading_engine::strategy::TradingState;

        let mut state = TradingState::new();

        // Add an order
        let order = Order {
            order_id: OrderId::new("test123".into()),
            symbol: Symbol::from("BTCUSDT"),
            side: core_logic::data::base_data::Side::Buy,
            price: rust_decimal::Decimal::new(50000, 0),
            amount: rust_decimal::Decimal::new(1, 3),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: rust_decimal::Decimal::ZERO,
        };

        state.add_order(order.clone());

        // Manually set a very old timestamp
        let old_timestamp = 0; // Very old
        state
            .pending_orders
            .insert(order.order_id.clone(), old_timestamp);

        // Get stale orders (with 1 second timeout)
        let stale = state.get_stale_orders(1);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0], order.order_id);
    }

    /// Test health server structure
    #[tokio::test]
    async fn test_health_server_structure() {
        use runner::health::HealthServer;
        use trading_engine::strategies::BuyLowSellHigh;

        let config = TradingSystemConfig::load_from_file("config/trading_system.json").unwrap();
        let binance_config_json = config.exchange.exchange_specific.get("binance").unwrap();

        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        let trading_client: Arc<
            RwLock<dyn TradingClient<core_logic::data::private_data::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(binance_config, 1024)));

        let strategy = Arc::new(RwLock::new(BuyLowSellHigh::new(
            trading_client,
            symbol,
            rust_decimal::Decimal::new(1, 3),
            rust_decimal::Decimal::new(1, 2),
            rust_decimal::Decimal::new(2, 0),
            rust_decimal::Decimal::new(2, 0),
            Some(rust_decimal::Decimal::new(5, 0)),
            Some(rust_decimal::Decimal::new(10, 0)),
            2,
            100,
            20,
        )));

        let _health_server = HealthServer::new(strategy);
        // If we get here, the server was created successfully
        assert!(true);
    }
}
