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
        let mut state_guard = self.state.write().await;
        if *state_guard != CircuitBreakerState::Closed {
            *state_guard = CircuitBreakerState::Closed;
            *self.last_open_time.write().await = None;
            *self.open_reason.write().await = None;
            *self.failure_count.write().await = 0;
            info!("Circuit breaker closed, resuming normal operation.");
        }
    }

    /// Record a successful operation
    pub async fn record_success(&self) {
        let mut failure_count_guard = self.failure_count.write().await;
        *failure_count_guard = 0; // Reset failure count on success
        let state_guard = self.state.read().await;
        if *state_guard == CircuitBreakerState::HalfOpen {
            drop(state_guard); // Drop read lock before acquiring write lock
            self.close().await;
        }
    }

    /// Record a failed operation
    pub async fn record_failure(&self) {
        let mut failure_count_guard = self.failure_count.write().await;
        *failure_count_guard += 1;
        warn!(
            "Circuit breaker: Failure recorded. Total failures: {}",
            *failure_count_guard
        );

        let state_guard = self.state.read().await;
        if *state_guard == CircuitBreakerState::HalfOpen {
            drop(state_guard); // Drop read lock before acquiring write lock
            self.open("Failure in Half-Open state").await;
        } else if *state_guard == CircuitBreakerState::Closed
            && *failure_count_guard >= self.max_failures
        {
            drop(state_guard); // Drop read lock before acquiring write lock
            self.open(&format!("Max failures ({}) reached", self.max_failures))
                .await;
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
