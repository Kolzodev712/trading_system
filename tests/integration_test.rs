// Integration tests for the trading system
// These tests verify that all components work together correctly

mod integration {
    use exchange_engine::exchanges::binance::{config::BinanceConfig, ws_client::BinanceClient};
    use runner::config::{BotConfig, StrategiesConfig};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use trading_core::{traits::TradingClient, types::common::Pair};
    use trading_engine::{state::Strategy, strategies::BuyLowSellHigh};

    /// Test that the system can load configuration
    #[tokio::test]
    async fn test_config_loading() {
        let bot_config = BotConfig::load_from_file("config/bot.json");
        assert!(bot_config.is_ok(), "Should load bot config file");

        let bot_config = bot_config.unwrap();
        assert_eq!(
            bot_config.exchange.exchange,
            trading_core::Exchange::Binance
        );

        let strategies_config = StrategiesConfig::load_from_file("config/strategies.json");
        assert!(
            strategies_config.is_ok(),
            "Should load strategies config file"
        );

        let strategies_config = strategies_config.unwrap();
        assert_eq!(strategies_config.strategy_type, "BuyLowSellHigh");
    }

    /// Test that configuration validation works
    #[tokio::test]
    async fn test_config_validation() {
        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let result = bot_config.validate();
        assert!(
            result.is_ok(),
            "Bot config should be valid: {:?}",
            result.err()
        );

        let strategies_config = StrategiesConfig::load_from_file("config/strategies.json").unwrap();
        let result = strategies_config.validate();
        assert!(
            result.is_ok(),
            "Strategies config should be valid: {:?}",
            result.err()
        );
    }

    /// Test that strategy can be created
    #[tokio::test]
    async fn test_strategy_creation() {
        // Create a mock trading client (we'll use a real one but won't connect)
        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();

        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

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
        use trading_core::types::{
            common::{OrderId, OrderStatus, Symbol},
            exchange::Order,
        };

        let mut state = trading_engine::state::TradingState::new();

        // Create a test order
        let order = Order {
            order_id: OrderId::new("test123".into()),
            symbol: Symbol::from("BTCUSDT"),
            side: trading_core::types::common::Side::Buy,
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
        use trading_core::circuit_breaker::CircuitBreaker;

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
        use trading_core::types::common::Pair;
        use trading_engine::state::TradingState;

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
        use trading_core::types::{
            common::{OrderId, OrderStatus, Symbol},
            exchange::Order,
        };
        use trading_engine::state::TradingState;

        let mut state = TradingState::new();

        // Add an order
        let order = Order {
            order_id: OrderId::new("test123".into()),
            symbol: Symbol::from("BTCUSDT"),
            side: trading_core::types::common::Side::Buy,
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
        use trading_engine::health::HealthServer;
        use trading_engine::strategies::BuyLowSellHigh;

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();

        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

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

        // Extract TradingState from strategy
        let trading_state = {
            let strat = strategy.read().await;
            Arc::new(RwLock::new(strat.get_state().clone()))
        };

        let _health_server = HealthServer::new(trading_state);
        // If we get here, the server was created successfully
        assert!(true);
    }

    /// Test strategy decision making with orderbook
    #[tokio::test]
    async fn test_strategy_orderbook_decision() {
        use trading_core::types::market_data::Orderbook;
        use trading_engine::strategies::BuyLowSellHigh;

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();
        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

        let mut strategy = BuyLowSellHigh::new(
            trading_client,
            symbol,
            rust_decimal::Decimal::new(1, 3), // 0.001
            rust_decimal::Decimal::new(1, 2), // 0.01
            rust_decimal::Decimal::new(2, 0), // 2.0%
            rust_decimal::Decimal::new(2, 0), // 2.0%
            Some(rust_decimal::Decimal::new(5, 0)),
            Some(rust_decimal::Decimal::new(10, 0)),
            2,
            100,
            20,
        );

        // Build price history by processing orderbooks
        let avg_price = rust_decimal::Decimal::new(50000, 0);
        let history_orderbook = Orderbook {
            bids: vec![avg_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![avg_price + rust_decimal::Decimal::new(1, 0)],
        };
        for _ in 0..25 {
            let state = strategy.get_state().clone();
            let _ = strategy.on_orderbook(&history_orderbook, &state).await;
        }

        // Create orderbook with price 3% below average (should trigger buy - threshold is 2%)
        // Using 3% to ensure we're well above the threshold
        let buy_price =
            avg_price * rust_decimal::Decimal::new(97, 0) / rust_decimal::Decimal::new(100, 0);
        let orderbook = Orderbook {
            bids: vec![buy_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![buy_price + rust_decimal::Decimal::new(1, 0)],
        };

        let state = strategy.get_state().clone();
        let result = strategy.on_orderbook(&orderbook, &state).await;

        // Should generate a buy order
        assert!(result.is_ok());
        let order_request = result.unwrap();
        assert!(
            order_request.is_some(),
            "Strategy should generate buy order when price is 3% below average"
        );
        assert_eq!(
            order_request.as_ref().unwrap().side,
            trading_core::types::common::Side::Buy
        );
    }

    /// Test strategy stop-loss logic
    #[tokio::test]
    async fn test_strategy_stop_loss() {
        use trading_core::types::market_data::Orderbook;
        use trading_engine::{state::Strategy, strategies::BuyLowSellHigh};

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();
        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

        let mut strategy = BuyLowSellHigh::new(
            trading_client,
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3),
            rust_decimal::Decimal::new(1, 2),
            rust_decimal::Decimal::new(2, 0),
            rust_decimal::Decimal::new(2, 0),
            Some(rust_decimal::Decimal::new(5, 0)), // 5% stop-loss
            Some(rust_decimal::Decimal::new(10, 0)),
            2,
            100,
            20,
        );

        // Build price history by processing orderbooks
        let entry_price = rust_decimal::Decimal::new(50000, 0);
        let history_orderbook = Orderbook {
            bids: vec![entry_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![entry_price + rust_decimal::Decimal::new(1, 0)],
        };
        for _ in 0..25 {
            let state = strategy.get_state().clone();
            let _ = strategy.on_orderbook(&history_orderbook, &state).await;
        }

        // Add a position with entry price
        strategy.get_state_mut().update_position(
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3), // 0.001 BTC
            entry_price,
        );

        // Create orderbook with price 6% below entry (should trigger stop-loss)
        let current_price =
            entry_price * rust_decimal::Decimal::new(94, 0) / rust_decimal::Decimal::new(100, 0);
        let orderbook = Orderbook {
            bids: vec![current_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![current_price + rust_decimal::Decimal::new(1, 0)],
        };

        let state = strategy.get_state().clone();
        let result = strategy.on_orderbook(&orderbook, &state).await;

        // Should generate a sell order (stop-loss)
        assert!(result.is_ok());
        let order_request = result.unwrap();
        assert!(order_request.is_some());
        assert_eq!(
            order_request.as_ref().unwrap().side,
            trading_core::types::common::Side::Sell
        );
    }

    /// Test strategy take-profit logic
    #[tokio::test]
    async fn test_strategy_take_profit() {
        use trading_core::types::market_data::Orderbook;
        use trading_engine::{state::Strategy, strategies::BuyLowSellHigh};

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();
        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

        let mut strategy = BuyLowSellHigh::new(
            trading_client,
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3),
            rust_decimal::Decimal::new(1, 2),
            rust_decimal::Decimal::new(2, 0),
            rust_decimal::Decimal::new(2, 0),
            Some(rust_decimal::Decimal::new(5, 0)),
            Some(rust_decimal::Decimal::new(10, 0)), // 10% take-profit
            2,
            100,
            20,
        );

        // Build price history by processing orderbooks
        let entry_price = rust_decimal::Decimal::new(50000, 0);
        let history_orderbook = Orderbook {
            bids: vec![entry_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![entry_price + rust_decimal::Decimal::new(1, 0)],
        };
        for _ in 0..25 {
            let state = strategy.get_state().clone();
            let _ = strategy.on_orderbook(&history_orderbook, &state).await;
        }

        // Add a position
        strategy.get_state_mut().update_position(
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3),
            entry_price,
        );

        // Create orderbook with price 11% above entry (should trigger take-profit)
        let current_price =
            entry_price * rust_decimal::Decimal::new(111, 0) / rust_decimal::Decimal::new(100, 0);
        let orderbook = Orderbook {
            bids: vec![current_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![current_price + rust_decimal::Decimal::new(1, 0)],
        };

        let state = strategy.get_state().clone();
        let result = strategy.on_orderbook(&orderbook, &state).await;

        // Should generate a sell order (take-profit)
        assert!(result.is_ok());
        let order_request = result.unwrap();
        assert!(order_request.is_some());
        assert_eq!(
            order_request.as_ref().unwrap().side,
            trading_core::types::common::Side::Sell
        );
    }

    /// Test retry logic
    #[tokio::test]
    async fn test_retry_logic() {
        use trading_core::retry::{order_retry_config, retry_with_backoff};

        let config = order_retry_config();
        let mut attempts = 0;

        // Simulate a function that succeeds on the second attempt
        let result = retry_with_backoff(&config, || {
            attempts += 1;
            Box::pin(async move {
                if attempts < 2 {
                    Err("Temporary error")
                } else {
                    Ok("Success")
                }
            })
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
        assert_eq!(attempts, 2);
    }

    /// Test position update logic
    #[tokio::test]
    async fn test_position_updates() {
        use trading_core::types::common::Pair;
        use trading_engine::state::TradingState;

        let mut state = TradingState::new();
        let symbol = Pair::new("BTC", "USDT");

        // Add initial position
        state.update_position(
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3), // 0.001 BTC
            rust_decimal::Decimal::new(50000, 0),
        );

        // Add more to position
        state.update_position(
            symbol.clone(),
            rust_decimal::Decimal::new(5, 4), // 0.0005 BTC
            rust_decimal::Decimal::new(51000, 0),
        );

        let position = state.get_position(&symbol).unwrap();
        assert_eq!(position.quantity, rust_decimal::Decimal::new(15, 4)); // 0.0015 BTC
                                                                          // Average price should be weighted: (0.001 * 50000 + 0.0005 * 51000) / 0.0015
        assert!(position.avg_price > rust_decimal::Decimal::new(50000, 0));
        assert!(position.avg_price < rust_decimal::Decimal::new(51000, 0));

        // Reduce position (sell)
        state.update_position(
            symbol.clone(),
            -rust_decimal::Decimal::new(1, 3), // -0.001 BTC
            rust_decimal::Decimal::new(52000, 0),
        );

        let position = state.get_position(&symbol);
        assert!(position.is_some());
        assert_eq!(position.unwrap().quantity, rust_decimal::Decimal::new(5, 4));
        // 0.0005 BTC remaining
    }

    /// Test max position limit
    #[tokio::test]
    async fn test_max_position_limit() {
        use trading_core::types::market_data::Orderbook;
        use trading_engine::{state::Strategy, strategies::BuyLowSellHigh};

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();
        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

        let mut strategy = BuyLowSellHigh::new(
            trading_client,
            symbol.clone(),
            rust_decimal::Decimal::new(1, 3), // 0.001 per trade
            rust_decimal::Decimal::new(1, 2), // 0.01 max position
            rust_decimal::Decimal::new(2, 0),
            rust_decimal::Decimal::new(2, 0),
            None,
            None,
            2,
            100,
            20,
        );

        // Build price history by processing orderbooks
        let avg_price = rust_decimal::Decimal::new(50000, 0);
        let history_orderbook = Orderbook {
            bids: vec![avg_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![avg_price + rust_decimal::Decimal::new(1, 0)],
        };
        for _ in 0..25 {
            let state = strategy.get_state().clone();
            let _ = strategy.on_orderbook(&history_orderbook, &state).await;
        }

        // Add position at max
        strategy.get_state_mut().update_position(
            symbol.clone(),
            rust_decimal::Decimal::new(1, 2), // 0.01 BTC (max)
            avg_price,
        );

        // Create orderbook that would trigger buy
        let buy_price =
            avg_price * rust_decimal::Decimal::new(98, 0) / rust_decimal::Decimal::new(100, 0);
        let orderbook = Orderbook {
            bids: vec![buy_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![buy_price + rust_decimal::Decimal::new(1, 0)],
        };

        let state = strategy.get_state().clone();
        let result = strategy.on_orderbook(&orderbook, &state).await;

        // Should NOT generate order (max position reached)
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Test active order limit
    #[tokio::test]
    async fn test_active_order_limit() {
        use trading_core::types::{
            common::{OrderId, OrderStatus, Symbol},
            exchange::Order,
            market_data::Orderbook,
        };
        use trading_engine::{state::Strategy, strategies::BuyLowSellHigh};

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();
        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

        let mut strategy = BuyLowSellHigh::new(
            trading_client,
            symbol,
            rust_decimal::Decimal::new(1, 3),
            rust_decimal::Decimal::new(1, 2),
            rust_decimal::Decimal::new(2, 0),
            rust_decimal::Decimal::new(2, 0),
            None,
            None,
            2, // max_active_orders = 2
            100,
            20,
        );

        // Build price history by processing orderbooks
        let avg_price = rust_decimal::Decimal::new(50000, 0);
        let history_orderbook = Orderbook {
            bids: vec![avg_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![avg_price + rust_decimal::Decimal::new(1, 0)],
        };
        for _ in 0..25 {
            let state = strategy.get_state().clone();
            let _ = strategy.on_orderbook(&history_orderbook, &state).await;
        }

        // Add 2 active orders (at limit)
        for i in 0..2 {
            let order = Order {
                order_id: OrderId::new(format!("order{}", i).into()),
                symbol: Symbol::from("BTCUSDT"),
                side: trading_core::types::common::Side::Buy,
                price: avg_price,
                amount: rust_decimal::Decimal::new(1, 3),
                internal_created_at: 0,
                exchange_created_at: None,
                status: OrderStatus::New,
                filled_amount: rust_decimal::Decimal::ZERO,
            };
            strategy.get_state_mut().add_order(order);
        }

        // Create orderbook that would trigger buy
        let buy_price =
            avg_price * rust_decimal::Decimal::new(98, 0) / rust_decimal::Decimal::new(100, 0);
        let orderbook = Orderbook {
            bids: vec![buy_price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![buy_price + rust_decimal::Decimal::new(1, 0)],
        };

        let state = strategy.get_state().clone();
        let result = strategy.on_orderbook(&orderbook, &state).await;

        // Should NOT generate order (max active orders reached)
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Test price history building
    #[tokio::test]
    async fn test_price_history_building() {
        use trading_core::types::market_data::Orderbook;
        use trading_engine::{state::Strategy, strategies::BuyLowSellHigh};

        let bot_config = BotConfig::load_from_file("config/bot.json").unwrap();
        let binance_config_json = bot_config
            .exchange
            .exchange_specific
            .get("binance")
            .unwrap();
        let binance_config: BinanceConfig =
            serde_json::from_value(binance_config_json.clone()).unwrap();

        let symbol = Pair::new("BTC", "USDT");
        // Use default channel config for tests
        let channels = trading_core::config::ChannelConfig::default();
        let trading_client: Arc<
            RwLock<dyn TradingClient<trading_core::types::exchange::OrderRequest>>,
        > = Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));

        let mut strategy = BuyLowSellHigh::new(
            trading_client,
            symbol,
            rust_decimal::Decimal::new(1, 3),
            rust_decimal::Decimal::new(1, 2),
            rust_decimal::Decimal::new(2, 0),
            rust_decimal::Decimal::new(2, 0),
            None,
            None,
            2,
            100,
            20, // min_price_history = 20
        );

        let price = rust_decimal::Decimal::new(50000, 0);
        let orderbook = Orderbook {
            bids: vec![price - rust_decimal::Decimal::new(1, 0)],
            asks: vec![price + rust_decimal::Decimal::new(1, 0)],
        };

        // Process 10 orderbooks (below minimum)
        for _ in 0..10 {
            let state = strategy.get_state().clone();
            let result = strategy.on_orderbook(&orderbook, &state).await.unwrap();
            assert!(
                result.is_none(),
                "Should not trade until price history is built"
            );
        }

        // Process 15 more (now at 25, above minimum)
        for _ in 0..15 {
            let state = strategy.get_state().clone();
            strategy.on_orderbook(&orderbook, &state).await.unwrap();
        }

        // Now should have enough history (check by trying to get a decision)
        let state = strategy.get_state().clone();
        let result = strategy.on_orderbook(&orderbook, &state).await;
        // Should not return None due to insufficient history
        assert!(result.is_ok());
    }
}
