//! Circuit breaker for emergency stops and recovery
use smol_str::SmolStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{error, info, warn};

/// State of the circuit breaker
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Normal operation, all requests allowed
    Closed,
    /// Circuit is open, all requests blocked
    Open,
    /// Circuit is half-open, limited requests allowed to test recovery
    HalfOpen,
}

/// Circuit breaker for emergency stops and recovery
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    last_open_time: Arc<RwLock<Option<Instant>>>,
    reset_timeout: Duration, // How long to wait before moving to HalfOpen
    failure_count: Arc<RwLock<usize>>,
    max_failures: usize,
    open_reason: Arc<RwLock<Option<SmolStr>>>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            last_open_time: Arc::new(RwLock::new(None)),
            reset_timeout: Duration::from_secs(300), // 5 minutes
            failure_count: Arc::new(RwLock::new(0)),
            max_failures: 5, // 5 consecutive failures to open
            open_reason: Arc::new(RwLock::new(None)),
        }
    }
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current state of the circuit breaker
    pub async fn get_state(&self) -> CircuitBreakerState {
        let state_guard = self.state.read().await;
        state_guard.clone()
    }

    /// Open the circuit breaker
    pub async fn open(&self, reason: &str) {
        let mut state_guard = self.state.write().await;
        if *state_guard != CircuitBreakerState::Open {
            *state_guard = CircuitBreakerState::Open;
            *self.last_open_time.write().await = Some(Instant::now());
            *self.open_reason.write().await = Some(SmolStr::new(reason));
            *self.failure_count.write().await = 0; // Reset failure count when opening
            error!("Circuit breaker opened: {}", reason);
        }
    }

    /// Close the circuit breaker
    pub async fn close(&self) {
        {
            let mut state_guard = self.state.write().await;
            if *state_guard == CircuitBreakerState::Closed {
                return;
            }
            *state_guard = CircuitBreakerState::Closed;
        }
        *self.last_open_time.write().await = None;
        *self.open_reason.write().await = None;
        *self.failure_count.write().await = 0;
        info!("Circuit breaker closed, resuming normal operation.");
    }

    /// Record a successful operation
    pub async fn record_success(&self) {
        {
            let mut failure_count_guard = self.failure_count.write().await;
            *failure_count_guard = 0; // Reset failure count on success
        }
        if self.get_state().await == CircuitBreakerState::HalfOpen {
            self.close().await;
        }
    }

    /// Record a failed operation
    pub async fn record_failure(&self) {
        let should_open;
        let open_reason;
        {
            let mut failure_count_guard = self.failure_count.write().await;
            *failure_count_guard += 1;
            warn!(
                "Circuit breaker: Failure recorded. Total failures: {}",
                *failure_count_guard
            );

            let state_guard = self.state.read().await;
            if *state_guard == CircuitBreakerState::HalfOpen {
                should_open = true;
                open_reason = "Failure in Half-Open state".to_string();
            } else if *state_guard == CircuitBreakerState::Closed
                && *failure_count_guard >= self.max_failures
            {
                should_open = true;
                open_reason = format!("Max failures ({}) reached", self.max_failures);
            } else {
                should_open = false;
                open_reason = String::new();
            }
        } // Drop all locks before calling open()

        if should_open {
            self.open(&open_reason).await;
        }
    }

    /// Check if a request is allowed by the circuit breaker
    pub async fn is_allowed(&self) -> bool {
        let state_guard = self.state.read().await;
        match *state_guard {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                let last_open_time_guard = self.last_open_time.read().await;
                if let Some(last_open_time) = *last_open_time_guard {
                    if last_open_time.elapsed() > self.reset_timeout {
                        drop(state_guard); // Drop read lock before acquiring write lock
                        let mut state_write_guard = self.state.write().await;
                        *state_write_guard = CircuitBreakerState::HalfOpen;
                        info!("Circuit breaker moved to Half-Open state, attempting recovery.");
                        true // Allow a single request to test recovery
                    } else {
                        false // Still in Open state, block request
                    }
                } else {
                    false // Should not happen if state is Open
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Allow a single request, subsequent requests will be blocked until state changes
                // This simple implementation allows all requests in HalfOpen, a more robust one
                // would use a counter or a single-flight mechanism.
                true
            }
        }
    }

    /// Check if circuit breaker is open (blocking)
    pub async fn is_open(&self) -> bool {
        let state_guard = self.state.read().await;
        *state_guard == CircuitBreakerState::Open
    }

    /// Get the reason why the circuit breaker is open
    pub async fn get_open_reason(&self) -> Option<SmolStr> {
        self.open_reason.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_new() {
        let cb = CircuitBreaker::new();
        let state = cb.get_state().await;
        assert_eq!(state, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_default() {
        let cb = CircuitBreaker::default();
        let state = cb.get_state().await;
        assert_eq!(state, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open() {
        let cb = CircuitBreaker::new();
        cb.open("Test reason").await;

        let state = cb.get_state().await;
        assert_eq!(state, CircuitBreakerState::Open);
        assert!(cb.is_open().await);

        let reason = cb.get_open_reason().await;
        assert!(reason.is_some());
        assert!(reason.unwrap().as_str().contains("Test reason"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_close() {
        let cb = CircuitBreaker::new();
        cb.open("Test").await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);

        cb.close().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
        assert!(!cb.is_open().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_record_success() {
        let cb = CircuitBreaker::new();

        // Record some failures first
        cb.record_failure().await;
        cb.record_failure().await;

        // Record success should reset failure count
        cb.record_success().await;

        // Should still be closed
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_record_failure_opens() {
        let cb = CircuitBreaker::new();

        // Record max failures
        for _ in 0..5 {
            cb.record_failure().await;
        }

        // Should be open now
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_is_allowed_closed() {
        let cb = CircuitBreaker::new();
        assert!(cb.is_allowed().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_is_allowed_open() {
        let cb = CircuitBreaker::new();
        cb.open("Test").await;

        // Should not be allowed immediately
        assert!(!cb.is_allowed().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let cb = CircuitBreaker::new();
        cb.open("Test").await;

        // Manually set to half-open for testing (normally happens after timeout)
        {
            let mut state = cb.state.write().await;
            *state = CircuitBreakerState::HalfOpen;
        }

        // Should be allowed in half-open
        assert!(cb.is_allowed().await);

        // Success should close it
        cb.record_success().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure_reopens() {
        let cb = CircuitBreaker::new();
        cb.open("Test").await;

        // Manually set to half-open
        {
            let mut state = cb.state.write().await;
            *state = CircuitBreakerState::HalfOpen;
        }

        // Failure should reopen it
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);
    }
}
