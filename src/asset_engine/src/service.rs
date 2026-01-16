//! Asset Service - main service for fetching and caching asset data
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use trading_core::{
    error::EngineError,
    types::{
        asset::{ExchangeMarketData, SymbolInfo, TradingRules},
        common::{Exchange, Pair},
    },
};

use crate::cache::AssetDataCache;

#[async_trait]
pub trait AssetDataClient: Send + Sync + std::fmt::Debug {
    /// Fetch and parse market data for the specified pairs
    /// Each exchange client handles its own parsing logic
    async fn fetch_market_data(&self, pairs: &[Pair]) -> Result<ExchangeMarketData, EngineError>;
}

/// Asset Service - orchestrates asset data fetching and caching
pub struct AssetService {
    cache: Arc<AssetDataCache>,
    exchange_clients: Arc<RwLock<HashMap<Exchange, Arc<dyn AssetDataClient>>>>,
}

impl AssetService {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(AssetDataCache::new(std::time::Duration::from_secs(300))), // 5 minutes TTL
            exchange_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an exchange client
    pub async fn register_exchange(
        &self,
        exchange: Exchange,
        client: Arc<dyn AssetDataClient>,
    ) -> Result<(), EngineError> {
        let mut clients = self.exchange_clients.write().await;
        clients.insert(exchange, client);
        Ok(())
    }

    /// Fetch comprehensive market data for pairs
    pub async fn fetch_market_data(
        &self,
        exchange: Exchange,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError> {
        // Check cache first
        if let Some(cached) = self.cache.get(&exchange).await {
            if !self.cache.is_stale(&exchange).await {
                info!("Using cached market data for {:?}", exchange);
                return Ok(cached);
            }
            info!("Cache is stale for {:?}, fetching fresh data", exchange);
        }

        // Fetch fresh data
        // Get the client Arc and clone it so we can use it after dropping the lock
        let client = {
            let clients = self.exchange_clients.read().await;
            clients
                .get(&exchange)
                .ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Exchange {:?} not registered",
                        exchange
                    ))
                })?
                .clone()
        };

        // Each exchange client handles its own parsing
        let market_data = client.fetch_market_data(pairs).await?;

        // Update cache
        self.cache.update(exchange, market_data.clone()).await;

        info!(
            "Fetched market data: {} symbols, {} trading rules",
            market_data.symbol_info.len(),
            market_data.trading_rules.len()
        );

        Ok(market_data)
    }

    /// Get trading rules for a pair
    pub async fn get_trading_rules(&self, exchange: Exchange, pair: &Pair) -> Option<TradingRules> {
        if let Some(market_data) = self.cache.get(&exchange).await {
            market_data.trading_rules.get(pair).cloned()
        } else {
            None
        }
    }

    /// Get symbol info for a pair
    pub async fn get_symbol_info(&self, exchange: Exchange, pair: &Pair) -> Option<SymbolInfo> {
        if let Some(market_data) = self.cache.get(&exchange).await {
            market_data.symbol_info.get(pair).cloned()
        } else {
            None
        }
    }
}

impl Default for AssetService {
    fn default() -> Self {
        Self::new()
    }
}
