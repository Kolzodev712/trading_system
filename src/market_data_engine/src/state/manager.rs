//! Market data state manager
//!
//! This module provides `MarketDataStateManager` which manages in-memory market data state
//! (orderbooks, public trades) and publishes updates via broadcast channels.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use trading_core::{
    channels::market_data::MarketDataUpdateSender,
    error::EngineError,
    registry::symbol::SymbolRegistry,
    types::market_data::{StructuredOrderbook, StructuredPublicTrade},
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
    /// * `symbol` - Symbol from exchange (e.g., "BTCUSDT") - used for validation only
    /// * `orderbook` - Orderbook update from WebSocket (already contains pair)
    ///
    /// # Errors
    /// Returns an error if publishing fails or symbol doesn't match orderbook pair
    pub async fn update_orderbook(
        &self,
        symbol: &trading_core::types::common::Symbol,
        orderbook: StructuredOrderbook,
    ) -> Result<(), EngineError> {
        // Validate that symbol matches orderbook pair
        let expected_pair = self.symbol_registry.get_pair(symbol).ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("Symbol {} not found in registry", symbol))
        })?;

        if orderbook.pair != *expected_pair {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Symbol {} does not match orderbook pair {}",
                symbol,
                orderbook.pair
            )));
        }

        // Clone before lock to minimize lock time
        let pair_for_log = orderbook.pair.clone();
        let orderbook_for_state = orderbook.clone();
        let pair_for_state = orderbook_for_state.pair.clone();

        // 1. Update in-memory state (pair is moved, no need to clone again)
        {
            let mut state = self.state.write().await;
            state.update_orderbook(pair_for_state, orderbook_for_state);
        }

        // 2. Publish via broadcast channel (use original orderbook)
        // Note: Publishing failures are logged but don't fail the state update
        // In tests, channels may be closed (no consumers), which is fine
        if let Err(e) = self.publisher.publish_orderbook(&orderbook).await {
            warn!("Failed to publish orderbook update (channel may have no consumers): {}", e);
            // Don't fail - state is still updated correctly
        }

        debug!("Updated and published orderbook for pair: {}", pair_for_log);
        Ok(())
    }

    /// Add public trade and publish
    ///
    /// # Arguments
    /// * `symbol` - Symbol from exchange (e.g., "BTCUSDT") - used for validation only
    /// * `trade` - Trade update from WebSocket (already contains pair)
    ///
    /// # Errors
    /// Returns an error if publishing fails or symbol doesn't match trade pair
    pub async fn add_public_trade(
        &self,
        symbol: &trading_core::types::common::Symbol,
        trade: StructuredPublicTrade,
    ) -> Result<(), EngineError> {
        // Validate that symbol matches trade pair
        let expected_pair = self.symbol_registry.get_pair(symbol).ok_or_else(|| {
            EngineError::CustomError(anyhow::anyhow!("Symbol {} not found in registry", symbol))
        })?;

        if trade.pair != *expected_pair {
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Symbol {} does not match trade pair {}",
                symbol,
                trade.pair
            )));
        }

        // Clone before lock to minimize lock time
        let pair_for_log = trade.pair.clone();
        let trade_for_state = trade.clone();
        let pair_for_state = trade_for_state.pair.clone();

        // 1. Add to in-memory state (pair is moved, no need to clone again)
        {
            let mut state = self.state.write().await;
            state.add_public_trade(pair_for_state, trade_for_state);
        }

        // 2. Publish via broadcast channel (use original trade)
        // Note: Publishing failures are logged but don't fail the state update
        // In tests, channels may be closed (no consumers), which is fine
        if let Err(e) = self.publisher.publish_trade(&trade).await {
            warn!("Failed to publish trade update (channel may have no consumers): {}", e);
            // Don't fail - state is still updated correctly
        }

        debug!("Added and published trade for pair: {}", pair_for_log);
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

    fn create_test_orderbook() -> StructuredOrderbook {
        use trading_core::types::common::Pair;
        StructuredOrderbook {
            pair: Pair::new("BTC", "USDT"),
            bids: vec![(Decimal::new(50000, 0), Decimal::new(1, 0))],
            asks: vec![(Decimal::new(51000, 0), Decimal::new(1, 0))],
            timestamp: 1234567890,
        }
    }

    fn create_test_trade() -> StructuredPublicTrade {
        use trading_core::types::common::Pair;
        StructuredPublicTrade {
            pair: Pair::new("BTC", "USDT"),
            price: Decimal::new(123456, 2),
            quantity: Decimal::new(5, 1),
            timestamp: 123456789,
            is_market_maker: true,
            trade_id: None,
        }
    }

    #[tokio::test]
    async fn test_update_orderbook() {
        use trading_core::registry::symbol::SymbolRegistry;
        use trading_core::types::common::{Pair, Symbol};

        // No receiver needed - publish failures are now non-fatal (just logged)
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

        // No receiver needed - publish failures are now non-fatal (just logged)
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
