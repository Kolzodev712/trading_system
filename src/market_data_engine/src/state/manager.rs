//! Market data state manager
//!
//! This module provides `MarketDataStateManager` which manages in-memory market data state
//! (orderbooks, public trades) and publishes updates via broadcast channels.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};
use trading_core::{
    channels::market_data::MarketDataUpdateSender,
    error::EngineError,
    registry::symbol::SymbolRegistry,
    types::market_data::{LegacyOrderbook as Orderbook, LegacyTrade as Trade},
};

use super::{publisher::MarketDataStatePublisher, state::MarketDataState};

/// Manages in-memory market data state (orderbooks, public trades)
///
/// This state manager:
/// - Updates in-memory state when orderbooks/trades arrive
/// - Publishes all updates via broadcast channels
/// - Uses SymbolRegistry for Symbol↔Pair conversions
/// - No reconciliation needed (market data is public)
#[derive(Debug)]
pub struct MarketDataStateManager {
    /// In-memory state (market data)
    state: Arc<RwLock<MarketDataState>>,

    /// Publisher for state updates via broadcast channels
    publisher: MarketDataStatePublisher,

    /// Symbol registry for Symbol↔Pair conversions
    symbol_registry: Arc<SymbolRegistry>,
}

impl MarketDataStateManager {
    /// Create a new MarketDataStateManager
    ///
    /// # Arguments
    /// * `market_data_tx` - Broadcast channel sender for publishing updates
    /// * `symbol_registry` - Symbol registry for Symbol↔Pair conversions
    pub fn new(
        market_data_tx: MarketDataUpdateSender,
        symbol_registry: Arc<SymbolRegistry>,
    ) -> Self {
        let state = Arc::new(RwLock::new(MarketDataState::new()));
        let publisher = MarketDataStatePublisher::new(market_data_tx);

        Self {
            state,
            publisher,
            symbol_registry,
        }
    }

    /// Update orderbook and publish
    ///
    /// # Arguments
    /// * `symbol` - Symbol from exchange (e.g., "BTCUSDT")
    /// * `orderbook` - Orderbook update from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails or symbol is not found in registry
    pub async fn update_orderbook(
        &self,
        symbol: &trading_core::types::common::Symbol,
        orderbook: Orderbook,
    ) -> Result<(), EngineError> {
        // Convert symbol to pair using registry
        let pair = self
            .symbol_registry
            .get_pair(symbol)
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Symbol {} not found in registry", symbol))
            })?
            .clone();

        // 1. Update in-memory state
        {
            let mut state = self.state.write().await;
            state.update_orderbook(pair.clone(), orderbook.clone());
        }

        // 2. Publish via broadcast channel
        if let Err(e) = self.publisher.publish_orderbook(&pair, &orderbook).await {
            error!("Failed to publish orderbook update: {}", e);
            return Err(e);
        }

        debug!("Updated and published orderbook for pair: {}", pair);
        Ok(())
    }

    /// Add public trade and publish
    ///
    /// # Arguments
    /// * `symbol` - Symbol from exchange (e.g., "BTCUSDT")
    /// * `trade` - Trade update from WebSocket
    ///
    /// # Errors
    /// Returns an error if publishing fails or symbol is not found in registry
    pub async fn add_public_trade(
        &self,
        symbol: &trading_core::types::common::Symbol,
        trade: Trade,
    ) -> Result<(), EngineError> {
        // Convert symbol to pair using registry
        let pair = self
            .symbol_registry
            .get_pair(symbol)
            .ok_or_else(|| {
                EngineError::CustomError(anyhow::anyhow!("Symbol {} not found in registry", symbol))
            })?
            .clone();

        // 1. Add to in-memory state
        {
            let mut state = self.state.write().await;
            state.add_public_trade(pair.clone(), trade.clone());
        }

        // 2. Publish via broadcast channel
        if let Err(e) = self.publisher.publish_trade(&pair, &trade).await {
            error!("Failed to publish trade update: {}", e);
            return Err(e);
        }

        debug!("Added and published trade for pair: {}", pair);
        Ok(())
    }

    /// Get current state
    ///
    /// Returns a shared reference to the state for external access
    pub fn get_state(&self) -> Arc<RwLock<MarketDataState>> {
        self.state.clone()
    }

    /// Get the symbol registry
    pub fn symbol_registry(&self) -> &Arc<SymbolRegistry> {
        &self.symbol_registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::channels::market_data::market_data_update_channel;

    fn create_test_orderbook() -> Orderbook {
        Orderbook {
            bids: vec![Decimal::new(50000, 0), Decimal::new(1, 0)],
            asks: vec![Decimal::new(51000, 0), Decimal::new(1, 0)],
        }
    }

    fn create_test_trade() -> Trade {
        Trade {
            price: Decimal::new(123456, 2),
            quantity: Decimal::new(5, 1),
            time: 123456789,
            is_market_maker: true,
        }
    }

    #[tokio::test]
    async fn test_update_orderbook() {
        use trading_core::registry::symbol::SymbolRegistry;
        use trading_core::types::common::{Pair, Symbol};

        let (tx, _) = market_data_update_channel(10);
        let pair = Pair::new("BTC", "USDT");
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair.clone()]));
        let manager = MarketDataStateManager::new(tx, symbol_registry);

        let orderbook = create_test_orderbook();
        let symbol: Symbol = "BTCUSDT".into();
        manager
            .update_orderbook(&symbol, orderbook.clone())
            .await
            .unwrap();

        let state_arc = manager.get_state();
        let state = state_arc.read().await;
        let stored_orderbook = state.get_orderbook(&pair).unwrap();
        assert_eq!(stored_orderbook.bids, orderbook.bids);
        assert_eq!(stored_orderbook.asks, orderbook.asks);
    }

    #[tokio::test]
    async fn test_add_public_trade() {
        use trading_core::registry::symbol::SymbolRegistry;
        use trading_core::types::common::{Pair, Symbol};

        let (tx, _) = market_data_update_channel(10);
        let pair = Pair::new("BTC", "USDT");
        let symbol_registry = Arc::new(SymbolRegistry::from_pairs(vec![pair.clone()]));
        let manager = MarketDataStateManager::new(tx, symbol_registry);

        let trade = create_test_trade();
        let symbol: Symbol = "BTCUSDT".into();
        manager
            .add_public_trade(&symbol, trade.clone())
            .await
            .unwrap();

        let state_arc = manager.get_state();
        let state = state_arc.read().await;
        let trades = state.get_public_trades(&pair);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, trade.price);
    }
}
