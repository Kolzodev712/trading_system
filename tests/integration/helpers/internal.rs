//! Test helpers and utilities for integration tests

use async_trait::async_trait;
use rust_decimal_macros::dec;
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::error::EngineError;
use trading_core::registry::symbol::SymbolRegistry;
use trading_core::traits::TradingClient;
use trading_core::types::common::{
    Instrument, InstrumentType, OrderId, OrderStatus, Pair, Side, Symbol,
};
use trading_core::types::exchange::{Balance, Order, OrderRequest, Position, Trade};
use trading_engine::levels::config::{
    LevelRiskConfig, LifecycleConfig, OrderTypeConfig, PriceConfig, QuantityConfig,
};
use trading_engine::levels::LevelConfig;
use trading_engine::risk::{RiskLimits, RiskManager};

/// Create a test level configuration
pub fn create_test_level_config(id: &str) -> LevelConfig {
    LevelConfig {
        id: SmolStr::new(id),
        name: None,
        side: Side::Buy,
        order_type: OrderTypeConfig::Limit,
        quantity: QuantityConfig::Fixed(dec!(0.001)),
        price: PriceConfig::Fixed(dec!(50000)),
        risk: LevelRiskConfig::default(),
        lifecycle: LifecycleConfig::default(),
        enabled: true,
        group: None,
    }
}

/// Create a test risk manager with default limits
pub fn create_test_risk_manager() -> RiskManager {
    let limits = RiskLimits::default();
    let symbol_registry = Arc::new(SymbolRegistry::new());
    RiskManager::new(limits, symbol_registry)
}

/// Create a test order
pub fn create_test_order(
    order_id: &str,
    price: rust_decimal::Decimal,
    amount: rust_decimal::Decimal,
) -> Order {
    Order {
        order_id: trading_core::types::common::OrderId::new(order_id.into()),
        client_order_id: None,
        instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
        side: Side::Buy,
        price,
        amount,
        internal_created_at: 0,
        exchange_created_at: None,
        status: trading_core::types::common::OrderStatus::New,
        filled_amount: dec!(0),
    }
}

/// Create a test order request
pub fn create_test_order_request(
    price: rust_decimal::Decimal,
    amount: rust_decimal::Decimal,
) -> OrderRequest {
    OrderRequest {
        instrument: Instrument::new(InstrumentType::Spot, Pair::new("BTC", "USDT")),
        side: Side::Buy,
        order_type: trading_core::types::common::OrderType::Limit,
        price,
        amount,
        timestamp: None,
        exchange_params: std::collections::HashMap::new(),
    }
}

/// Create a test balance
pub fn create_test_balance(
    currency: &str,
    free: rust_decimal::Decimal,
    used: rust_decimal::Decimal,
) -> Balance {
    Balance {
        symbol: Symbol::from(currency),
        free,
        used,
        total: free + used,
    }
}

/// Create Binance config for testing
pub fn create_test_binance_config() -> exchange_engine::exchanges::binance::config::BinanceConfig {
    use trading_core::secret::SecretString;

    exchange_engine::exchanges::binance::config::BinanceConfig {
        pairs: vec![Pair::new("BTC", "USDT")],
        api_key: SecretString::from("key"),
        secret: SecretString::from("secret"),
        limit: None,
        base_delay: None,
        max_delay: None,
    }
}

/// Create exchange state manager for testing
pub async fn create_test_exchange_state_manager() -> (
    exchange_engine::state::manager::ExchangeStateManager,
    trading_core::channels::exchange::ExchangeUpdateReceiver,
) {
    use exchange_engine::exchanges::binance::client::BinanceClient;
    use trading_core::channels::exchange::exchange_update_channel;
    use trading_core::config::ChannelConfig;
    use trading_core::registry::symbol::SymbolRegistry;

    let (tx, rx) = exchange_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::new());
    let channels = ChannelConfig::default();
    let binance_config = create_test_binance_config();
    let trading_client: Arc<RwLock<dyn trading_core::traits::TradingClient<OrderRequest>>> =
        Arc::new(RwLock::new(BinanceClient::new(
            binance_config,
            channels.websocket_inbound_capacity,
            channels.websocket_ping_capacity,
            channels.websocket_outbound_rpc_capacity,
            channels.shutdown_capacity,
        )));
    let manager = exchange_engine::state::manager::ExchangeStateManager::new(
        tx,
        symbol_registry,
        trading_client,
        0,
        false,
    );
    (manager, rx)
}

/// Create market data state manager for testing
pub fn create_test_market_data_manager() -> (
    market_data_engine::state::manager::MarketDataStateManager,
    trading_core::channels::market_data::MarketDataUpdateReceiver,
) {
    use trading_core::channels::market_data::market_data_update_channel;
    use trading_core::registry::symbol::SymbolRegistry;
    use trading_core::types::common::Pair;

    let (tx, rx) = market_data_update_channel(100);
    let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![Pair::new("BTC", "USDT")]));
    let manager =
        market_data_engine::state::manager::MarketDataStateManager::new(tx, symbol_registry);
    (manager, rx)
}

/// Mock TradingClient for integration tests
/// Implements futures methods to test futures trading functionality
#[derive(Debug, Clone)]
pub struct MockTradingClient {
    balances: Vec<Balance>,
    open_orders: Vec<Order>,
    trades: Vec<Trade>,
    // Futures-specific data
    positions: Vec<Position>,
    collateral_balances: Vec<trading_core::types::futures::FuturesBalance>,
    leverage_info: std::collections::HashMap<Pair, trading_core::types::futures::LeverageInfo>,
    position_mode: Option<trading_core::types::futures::PositionMode>,
    margin_types: std::collections::HashMap<Pair, trading_core::types::futures::MarginType>,
    futures_orders: Vec<Order>,
}

impl MockTradingClient {
    pub fn new() -> Self {
        Self {
            balances: vec![],
            open_orders: vec![],
            trades: vec![],
            positions: vec![],
            collateral_balances: vec![],
            leverage_info: std::collections::HashMap::new(),
            position_mode: None,
            margin_types: std::collections::HashMap::new(),
            futures_orders: vec![],
        }
    }

    /// Configure mock with specific balances for testing
    pub fn with_balances(mut self, balances: Vec<Balance>) -> Self {
        self.balances = balances;
        self
    }

    /// Configure mock with specific open orders for testing
    pub fn with_open_orders(mut self, orders: Vec<Order>) -> Self {
        self.open_orders = orders;
        self
    }

    /// Configure mock with specific trades for testing
    pub fn with_trades(mut self, trades: Vec<Trade>) -> Self {
        self.trades = trades;
        self
    }

    /// Configure mock with specific positions for testing
    pub fn with_positions(mut self, positions: Vec<Position>) -> Self {
        self.positions = positions;
        self
    }

    /// Configure mock with specific collateral balances for testing
    pub fn with_collateral_balances(
        mut self,
        balances: Vec<trading_core::types::futures::FuturesBalance>,
    ) -> Self {
        self.collateral_balances = balances;
        self
    }

    /// Configure mock with leverage info for a pair
    pub fn with_leverage_info(
        mut self,
        pair: Pair,
        leverage_info: trading_core::types::futures::LeverageInfo,
    ) -> Self {
        self.leverage_info.insert(pair, leverage_info);
        self
    }

    /// Configure mock with position mode
    pub fn with_position_mode(mut self, mode: trading_core::types::futures::PositionMode) -> Self {
        self.position_mode = Some(mode);
        self
    }

    /// Configure mock with margin type for a pair
    pub fn with_margin_type(
        mut self,
        pair: Pair,
        margin_type: trading_core::types::futures::MarginType,
    ) -> Self {
        self.margin_types.insert(pair, margin_type);
        self
    }

    /// Configure mock with futures orders
    pub fn with_futures_orders(mut self, orders: Vec<Order>) -> Self {
        self.futures_orders = orders;
        self
    }
}

impl Default for MockTradingClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TradingClient<OrderRequest> for MockTradingClient {
    fn exchange_id(&self) -> trading_core::types::common::Exchange {
        trading_core::types::common::Exchange::Binance
    }

    async fn test_open_order(&self, _req: OrderRequest) -> Result<(), EngineError> {
        Ok(())
    }

    async fn open_order(&self, req: OrderRequest) -> Result<Order, EngineError> {
        Ok(Order {
            order_id: OrderId::new(format!("mock_order_{}", req.instrument.symbol()).into()),
            client_order_id: None,
            instrument: req.instrument.clone(),
            side: req.side,
            price: req.price,
            amount: req.amount,
            internal_created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: rust_decimal::Decimal::ZERO,
        })
    }

    async fn cancel_order_by_id(&self, _order_id: OrderId) -> Result<(), EngineError> {
        Ok(())
    }

    async fn cancel_all_orders(&self, _pair: &Pair) -> Result<(), EngineError> {
        Ok(())
    }

    async fn fetch_balance(&self) -> Result<Vec<Balance>, EngineError> {
        Ok(self.balances.clone())
    }

    async fn fetch_open_orders(&self) -> Result<Vec<Order>, EngineError> {
        Ok(self.open_orders.clone())
    }

    async fn fetch_trades(&self, pair: Option<&Pair>) -> Result<Vec<Trade>, EngineError> {
        let trades = if let Some(pair) = pair {
            self.trades
                .iter()
                .filter(|t| &t.pair == pair)
                .cloned()
                .collect()
        } else {
            self.trades.clone()
        };
        Ok(trades)
    }

    async fn fetch_market_data_snapshot(&self, _pair: &Pair) -> Result<(), EngineError> {
        Ok(())
    }

    async fn amend_order(
        &self,
        _order_id: OrderId,
        new_request: OrderRequest,
    ) -> Result<Order, EngineError> {
        Ok(Order {
            order_id: OrderId::new(format!("amended_{}", new_request.instrument.symbol()).into()),
            client_order_id: None,
            instrument: new_request.instrument.clone(),
            side: new_request.side,
            price: new_request.price,
            amount: new_request.amount,
            internal_created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: rust_decimal::Decimal::ZERO,
        })
    }

    // Futures methods
    async fn fetch_positions(&self) -> Result<Vec<Position>, EngineError> {
        Ok(self.positions.clone())
    }

    async fn fetch_position(&self, pair: &Pair) -> Result<Option<Position>, EngineError> {
        Ok(self
            .positions
            .iter()
            .find(|p| &p.instrument.pair == pair)
            .cloned())
    }

    async fn fetch_collateral_balance(
        &self,
    ) -> Result<Vec<trading_core::types::futures::FuturesBalance>, EngineError> {
        Ok(self.collateral_balances.clone())
    }

    async fn set_leverage(
        &self,
        pair: &Pair,
        leverage: u32,
    ) -> Result<trading_core::types::futures::LeverageInfo, EngineError> {
        let leverage_info = trading_core::types::futures::LeverageInfo {
            pair: pair.clone(),
            leverage,
            max_leverage: 125,
            last_update: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        Ok(leverage_info)
    }

    async fn get_leverage(
        &self,
        pair: &Pair,
    ) -> Result<trading_core::types::futures::LeverageInfo, EngineError> {
        Ok(self.leverage_info.get(pair).cloned().unwrap_or_else(|| {
            trading_core::types::futures::LeverageInfo {
                pair: pair.clone(),
                leverage: 1,
                max_leverage: 125,
                last_update: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            }
        }))
    }

    async fn set_position_mode(
        &self,
        _mode: trading_core::types::futures::PositionMode,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    async fn get_position_mode(
        &self,
    ) -> Result<trading_core::types::futures::PositionMode, EngineError> {
        Ok(self
            .position_mode
            .unwrap_or(trading_core::types::futures::PositionMode::OneWay))
    }

    async fn set_margin_type(
        &self,
        _pair: &Pair,
        _margin_type: trading_core::types::futures::MarginType,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    async fn get_margin_type(
        &self,
        pair: &Pair,
    ) -> Result<trading_core::types::futures::MarginType, EngineError> {
        Ok(self
            .margin_types
            .get(pair)
            .copied()
            .unwrap_or(trading_core::types::futures::MarginType::Cross))
    }

    async fn place_futures_order(
        &self,
        req: trading_core::types::futures::FuturesOrderRequest,
    ) -> Result<Order, EngineError> {
        Ok(Order {
            order_id: OrderId::new(
                format!("futures_order_{}", req.base.instrument.symbol()).into(),
            ),
            client_order_id: None,
            instrument: req.base.instrument.clone(),
            side: req.base.side,
            price: req.base.price,
            amount: req.base.amount,
            internal_created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: rust_decimal::Decimal::ZERO,
        })
    }

    async fn cancel_all_futures_orders(&self, pair: &Pair) -> Result<(), EngineError> {
        let _ = pair; // Suppress unused warning
        Ok(())
    }

    async fn fetch_open_futures_orders(
        &self,
        _pair: Option<&Pair>,
    ) -> Result<Vec<Order>, EngineError> {
        Ok(self
            .futures_orders
            .iter()
            .filter(|o| o.instrument.instrument_type() == InstrumentType::Futures)
            .cloned()
            .collect())
    }
}

/// Mock AssetDataClient for integration tests
#[derive(Debug)]
pub struct MockAssetDataClient {
    market_data: trading_core::types::asset::ExchangeMarketData,
}

#[async_trait]
impl asset_engine::AssetDataClient for MockAssetDataClient {
    async fn fetch_market_data(
        &self,
        pairs: &[trading_core::types::common::Pair],
    ) -> Result<trading_core::types::asset::ExchangeMarketData, EngineError> {
        // Filter market data to only include requested pairs
        let mut filtered_market_data = self.market_data.clone();

        // Filter symbol_info
        filtered_market_data
            .symbol_info
            .retain(|pair, _| pairs.contains(pair));

        // Filter trading_rules
        filtered_market_data
            .trading_rules
            .retain(|pair, _| pairs.contains(pair));

        Ok(filtered_market_data)
    }
}

impl MockAssetDataClient {
    pub fn new(market_data: trading_core::types::asset::ExchangeMarketData) -> Self {
        Self { market_data }
    }
}

/// Create mock market data for testing
pub fn create_mock_market_data() -> trading_core::types::asset::ExchangeMarketData {
    use std::collections::HashMap;
    use trading_core::types::asset::{
        ExchangeLimits, RateLimit, RateLimitType, SymbolInfo, SymbolStatus, TradingRules,
    };

    let pair = Pair::new("BTC", "USDT");
    let mut symbol_info = HashMap::new();
    let mut trading_rules = HashMap::new();

    symbol_info.insert(
        pair.clone(),
        SymbolInfo {
            pair: pair.clone(),
            status: SymbolStatus::Trading,
            base_asset: smol_str::SmolStr::new("BTC"),
            quote_asset: smol_str::SmolStr::new("USDT"),
            exchange_symbol: smol_str::SmolStr::new("BTCUSDT"),
            trading_allowed: true,
            is_active: true,
        },
    );

    trading_rules.insert(
        pair.clone(),
        TradingRules {
            pair: pair.clone(),
            min_order_size: Some(dec!(0.00001)),
            max_order_size: Some(dec!(9000)),
            step_size: Some(dec!(0.00001)),
            min_price: Some(dec!(0.01)),
            max_price: Some(dec!(1000000)),
            tick_size: Some(dec!(0.01)),
            min_notional: Some(dec!(10)),
            max_notional: None,
            price_precision: Some(8),
            quantity_precision: Some(8),
        },
    );

    trading_core::types::asset::ExchangeMarketData {
        symbol_info,
        trading_rules,
        current_prices: HashMap::new(),
        exchange_limits: ExchangeLimits {
            rate_limits: vec![RateLimit {
                rate_limit_type: RateLimitType::RequestWeight,
                interval: "MINUTE".to_string(),
                limit: 1200,
            }],
            max_position_size: None,
            max_open_orders: None,
            other_limits: HashMap::new(),
        },
        last_updated: 1234567890,
    }
}
