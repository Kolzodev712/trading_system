use crate::exchanges::bybit::config::BybitRateLimiterConfig;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use trading_core::rate_limiter::{RateLimiter, RollingWindowRateLimiter};

#[derive(Debug, Clone)]
struct EndpointLimit {
    limiter: Arc<dyn RateLimiter + Send + Sync>,
}

impl EndpointLimit {
    fn new(limit: u64) -> Self {
        Self {
            limiter: Arc::new(RollingWindowRateLimiter::new(limit, Duration::from_secs(1))),
        }
    }
}

#[derive(Debug)]
pub struct BybitRateLimiter {
    endpoint_limiters: Arc<RwLock<HashMap<SmolStr, EndpointLimit>>>,
    ip_http_limiter: Arc<dyn RateLimiter + Send + Sync>,
    endpoint_defaults: HashMap<SmolStr, u64>,
    default_endpoint_limit: u64,
}

impl BybitRateLimiter {
    pub fn new(config: BybitRateLimiterConfig) -> Self {
        let endpoint_defaults: HashMap<SmolStr, u64> = config
            .endpoint_limits
            .into_iter()
            .map(|(k, v)| (SmolStr::new(k), v))
            .collect();

        Self {
            endpoint_limiters: Arc::new(RwLock::new(HashMap::new())),
            ip_http_limiter: Arc::new(RollingWindowRateLimiter::new(
                config.ip_limit,
                Duration::from_secs(config.ip_window_seconds),
            )),
            endpoint_defaults,
            default_endpoint_limit: config.default_endpoint_limit,
        }
    }

    async fn get_or_create_endpoint_limiter(
        &self,
        endpoint: &str,
    ) -> Arc<dyn RateLimiter + Send + Sync> {
        let endpoint_key = SmolStr::new(endpoint);
        let mut limiters = self.endpoint_limiters.write().await;

        if let Some(limit) = limiters.get(&endpoint_key) {
            return limit.limiter.clone();
        }

        let default_limit = self
            .endpoint_defaults
            .get(&endpoint_key)
            .copied()
            .unwrap_or(self.default_endpoint_limit);

        let endpoint_limit = EndpointLimit::new(default_limit);
        let limiter = endpoint_limit.limiter.clone();

        limiters.insert(endpoint_key, endpoint_limit);

        limiter
    }

    pub async fn update_endpoint_limit(&self, endpoint: &str, new_limit: u64) {
        let endpoint_key = SmolStr::new(endpoint);
        let mut limiters = self.endpoint_limiters.write().await;

        if let Some(endpoint_limit) = limiters.get_mut(&endpoint_key) {
            endpoint_limit.limiter.update_limit(new_limit).await;
        } else {
            let endpoint_limit = EndpointLimit::new(new_limit);
            limiters.insert(endpoint_key, endpoint_limit);
        }
    }

    pub async fn wait_if_needed_endpoint(&self, endpoint: &str) {
        let limiter = self.get_or_create_endpoint_limiter(endpoint).await;
        limiter.wait_if_needed(1).await;
    }

    pub async fn wait_if_needed_batch_endpoint(&self, endpoint: &str, count: u64) {
        let limiter = self.get_or_create_endpoint_limiter(endpoint).await;
        limiter.wait_if_needed(count).await;
    }

    pub async fn record_endpoint(&self, endpoint: &str) {
        let limiter = self.get_or_create_endpoint_limiter(endpoint).await;
        limiter.record(1).await;
    }

    pub async fn record_batch_endpoint(&self, endpoint: &str, order_count: u64) {
        let limiter = self.get_or_create_endpoint_limiter(endpoint).await;
        limiter.record(order_count).await;
    }

    pub async fn wait_if_needed_ip_http(&self) {
        self.ip_http_limiter.wait_if_needed(1).await;
    }

    pub async fn record_ip_http(&self) {
        self.ip_http_limiter.record(1).await;
    }

    pub async fn get_endpoint_count(&self, endpoint: &str) -> u64 {
        let limiter = self.get_or_create_endpoint_limiter(endpoint).await;
        limiter.current_count().await
    }

    pub async fn get_endpoint_limit(&self, endpoint: &str) -> Option<u64> {
        let limiter = self.get_or_create_endpoint_limiter(endpoint).await;
        Some(limiter.current_limit().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchanges::bybit::config::BybitRateLimiterConfig;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_bybit_rate_limiter_endpoint() {
        let mut limits = HashMap::new();
        limits.insert("/v5/order/create".to_string(), 10);
        let config = BybitRateLimiterConfig {
            endpoint_limits: limits,
            ..Default::default()
        };
        let limiter = BybitRateLimiter::new(config);

        for _ in 0..10 {
            limiter.record_endpoint("/v5/order/create").await;
        }

        let count = limiter.get_endpoint_count("/v5/order/create").await;
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_bybit_rate_limiter_batch() {
        let mut limits = HashMap::new();
        limits.insert("/v5/order/create-batch".to_string(), 10);
        let config = BybitRateLimiterConfig {
            endpoint_limits: limits,
            ..Default::default()
        };
        let limiter = BybitRateLimiter::new(config);

        limiter
            .record_batch_endpoint("/v5/order/create-batch", 5)
            .await;

        let count = limiter.get_endpoint_count("/v5/order/create-batch").await;
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_bybit_rate_limiter_update_limit() {
        let config = BybitRateLimiterConfig::default();
        let limiter = BybitRateLimiter::new(config);

        limiter.update_endpoint_limit("/v5/order/create", 20).await;

        let limit = limiter.get_endpoint_limit("/v5/order/create").await;
        assert_eq!(limit, Some(20));
    }
}
