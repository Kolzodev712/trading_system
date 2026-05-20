//! Generic connection manager utilities for exchange drivers
//!
//! This module provides exchange-agnostic utilities for connection lifecycle management,
//! including state tracking, rate limiting, and session recovery scheduling.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info};
use trading_core::error::EngineError;
use trading_core::venue::cex::traits::ConnectionState;

/// Connection state manager
#[derive(Debug, Clone)]
pub struct ConnectionStateManager {
    state: Arc<RwLock<ConnectionState>>,
}

impl Default for ConnectionStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionStateManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
        }
    }

    pub async fn set_state(&self, new_state: ConnectionState) {
        let mut guard = self.state.write().await;
        let old_state = *guard;
        *guard = new_state;
        drop(guard);

        if old_state != new_state {
            info!(
                "Connection state changed: {:?} -> {:?}",
                old_state, new_state
            );
        }
    }

    pub async fn get_state(&self) -> ConnectionState {
        *self.state.read().await
    }
}

/// Rate limiter using token bucket algorithm
#[derive(Debug)]
pub struct RateLimiter {
    tokens: Arc<RwLock<u32>>,
    max_tokens: u32,
    refill_interval: Duration,
    refill_amount: u32,
    last_refill: Arc<RwLock<Instant>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `max_tokens` - Maximum number of tokens (requests) allowed
    /// * `refill_interval` - How often tokens are refilled
    /// * `refill_amount` - How many tokens to add per refill
    pub fn new(max_tokens: u32, refill_interval: Duration, refill_amount: u32) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(max_tokens)),
            max_tokens,
            refill_interval,
            refill_amount,
            last_refill: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Try to acquire a token (non-blocking)
    ///
    /// # Returns
    /// `true` if token was acquired, `false` if rate limit exceeded
    pub async fn try_acquire(&self) -> bool {
        self.refill_if_needed().await;

        let mut tokens = self.tokens.write().await;
        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }

    /// Wait until a token is available (blocking)
    pub async fn acquire(&self) {
        loop {
            if self.try_acquire().await {
                return;
            }
            tokio::time::sleep(self.refill_interval / 10).await;
        }
    }

    async fn refill_if_needed(&self) {
        let mut last_refill = self.last_refill.write().await;
        let now = Instant::now();

        if now.duration_since(*last_refill) >= self.refill_interval {
            let mut tokens = self.tokens.write().await;
            *tokens = (*tokens + self.refill_amount).min(self.max_tokens);
            *last_refill = now;
            drop(tokens);
        }
        drop(last_refill);
    }

    pub async fn available_tokens(&self) -> u32 {
        self.refill_if_needed().await;
        *self.tokens.read().await
    }
}

/// Session refresh scheduler
pub struct SessionRefreshScheduler {
    last_refresh: Arc<RwLock<Option<Instant>>>,
    refresh_interval: Duration,
}

impl SessionRefreshScheduler {
    pub fn new(refresh_interval: Duration) -> Self {
        Self {
            last_refresh: Arc::new(RwLock::new(None)),
            refresh_interval,
        }
    }

    pub async fn needs_refresh(&self) -> bool {
        if let Some(last) = *self.last_refresh.read().await {
            last.elapsed() >= self.refresh_interval
        } else {
            // Never refreshed, check if enough time has passed
            false
        }
    }

    pub async fn mark_refreshed(&self) {
        *self.last_refresh.write().await = Some(Instant::now());
    }

    pub fn spawn_refresh_task<F, Fut>(&self, mut refresh_fn: F) -> tokio::task::JoinHandle<()>
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), EngineError>> + Send + 'static,
    {
        let scheduler = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(scheduler.refresh_interval);
            loop {
                interval.tick().await;
                if scheduler.needs_refresh().await {
                    match refresh_fn().await {
                        Ok(_) => {
                            info!("Session refreshed successfully");
                            scheduler.mark_refreshed().await;
                        }
                        Err(e) => {
                            error!("Failed to refresh session: {}", e);
                        }
                    }
                }
            }
        })
    }
}

impl Clone for SessionRefreshScheduler {
    fn clone(&self) -> Self {
        Self {
            last_refresh: Arc::clone(&self.last_refresh),
            refresh_interval: self.refresh_interval,
        }
    }
}
