//! TTL-based caching for asset data
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use trading_core::types::{asset::ExchangeMarketData, common::Exchange};

/// Cache metadata for TTL tracking
pub struct CacheMetadata {
    cached_at: Instant,
    ttl: Duration,
}

impl CacheMetadata {
    pub fn new(ttl: Duration) -> Self {
        Self {
            cached_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_stale(&self) -> bool {
        self.cached_at.elapsed() >= self.ttl
    }
}

/// TTL-based cache for asset data
pub struct AssetDataCache {
    data: Arc<RwLock<HashMap<Exchange, ExchangeMarketData>>>,
    cache_metadata: Arc<RwLock<HashMap<Exchange, CacheMetadata>>>,
    default_ttl: Duration,
}

impl AssetDataCache {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            cache_metadata: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Get cached data if available and not stale
    pub async fn get(&self, exchange: &Exchange) -> Option<ExchangeMarketData> {
        let metadata_guard = self.cache_metadata.read().await;
        if let Some(metadata) = metadata_guard.get(exchange) {
            if !metadata.is_stale() {
                let data_guard = self.data.read().await;
                return data_guard.get(exchange).cloned();
            }
        }
        None
    }

    /// Update cache
    pub async fn update(&self, exchange: Exchange, data: ExchangeMarketData) {
        let mut data_guard = self.data.write().await;
        data_guard.insert(exchange.clone(), data);

        let mut metadata_guard = self.cache_metadata.write().await;
        metadata_guard.insert(exchange, CacheMetadata::new(self.default_ttl));
    }

    /// Check if cache is stale for an exchange
    pub async fn is_stale(&self, exchange: &Exchange) -> bool {
        let metadata_guard = self.cache_metadata.read().await;
        metadata_guard
            .get(exchange)
            .map(|m| m.is_stale())
            .unwrap_or(true)
    }
}
