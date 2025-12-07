//! Retry logic with exponential backoff
use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Retry a function with exponential backoff
pub async fn retry_with_backoff<F, T, E>(config: &RetryConfig, mut f: F) -> Result<T, E>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
    E: std::fmt::Debug,
{
    let mut delay = config.initial_delay_ms;
    let mut last_error = None;

    for attempt in 1..=config.max_attempts {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < config.max_attempts {
                    let sleep_duration = Duration::from_millis(
                        (delay as f64 * config.backoff_multiplier.powi(attempt as i32 - 1))
                            .min(config.max_delay_ms as f64) as u64,
                    );
                    sleep(sleep_duration).await;
                    delay = (delay as f64 * config.backoff_multiplier) as u64;
                }
            }
        }
    }

    Err(last_error.expect("Should have at least one error"))
}

/// Retry configuration for order operations
pub fn order_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 500,
        max_delay_ms: 2000,
        backoff_multiplier: 2.0,
    }
}
