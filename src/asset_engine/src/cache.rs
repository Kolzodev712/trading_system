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
        let data_clone = data.clone();
        let exchange_clone = exchange.clone();
        {
            let mut data_guard = self.data.write().await;
            data_guard.insert(exchange_clone, data_clone);
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use trading_core::types::asset::ExchangeMarketData;

    fn create_test_market_data() -> ExchangeMarketData {
        ExchangeMarketData {
            symbol_info: std::collections::HashMap::new(),
            trading_rules: std::collections::HashMap::new(),
            current_prices: std::collections::HashMap::new(),
            exchange_limits: trading_core::types::asset::ExchangeLimits {
                rate_limits: vec![],
                max_position_size: None,
                max_open_orders: None,
                other_limits: std::collections::HashMap::new(),
            },
            last_updated: 0,
        }
    }

    #[tokio::test]
    async fn test_cache_new() {
        let cache = AssetDataCache::new(Duration::from_secs(60));
        assert!(cache.get(&Exchange::Binance).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_update_and_get() {
        let cache = AssetDataCache::new(Duration::from_secs(60));
        let exchange = Exchange::Binance;
        let market_data = create_test_market_data();

        cache.update(exchange.clone(), market_data.clone()).await;

        let retrieved = cache.get(&exchange).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().last_updated, market_data.last_updated);
    }

    #[tokio::test]
    async fn test_cache_stale() {
        let cache = AssetDataCache::new(Duration::from_millis(10));
        let exchange = Exchange::Binance;
        let market_data = create_test_market_data();

        cache.update(exchange.clone(), market_data).await;

        // Should not be stale immediately
        assert!(!cache.is_stale(&exchange).await);

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(15)).await;

        // Should be stale now
        assert!(cache.is_stale(&exchange).await);

        // Get should return None for stale data
        assert!(cache.get(&exchange).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_multiple_exchanges() {
        let cache = AssetDataCache::new(Duration::from_secs(60));
        let binance_data = create_test_market_data();
        let mut test_data = create_test_market_data();
        test_data.last_updated = 1;

        // Use Binance twice with different data to test multiple entries
        cache.update(Exchange::Binance, binance_data.clone()).await;

        // Update with different data
        cache.update(Exchange::Binance, test_data.clone()).await;

        let retrieved = cache.get(&Exchange::Binance).await;

        assert!(retrieved.is_some());
        // Should have the latest data
        assert_eq!(retrieved.unwrap().last_updated, test_data.last_updated);
    }

    #[tokio::test]
    async fn test_cache_metadata_is_stale() {
        let metadata = CacheMetadata::new(Duration::from_millis(10));
        assert!(!metadata.is_stale());

        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(metadata.is_stale());
    }
}
