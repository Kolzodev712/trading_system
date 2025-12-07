//! Circuit breaker pattern for handling repeated failures
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Circuit breaker pattern for handling repeated failures
pub struct CircuitBreaker {
    failure_count: AtomicU32,
    last_failure_time: std::sync::Mutex<Option<Instant>>,
    failure_threshold: u32,
    timeout: Duration,
    #[allow(dead_code)] // Reserved for future use
    half_open_timeout: Duration,
    state: std::sync::Mutex<CircuitState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, blocking requests
    HalfOpen, // Testing if service recovered
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, timeout_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            last_failure_time: std::sync::Mutex::new(None),
            failure_threshold,
            timeout: Duration::from_secs(timeout_secs),
            half_open_timeout: Duration::from_secs(timeout_secs / 2),
            state: std::sync::Mutex::new(CircuitState::Closed),
        }
    }

    /// Check if request should be allowed
    pub fn can_proceed(&self) -> bool {
        let state = *self.state.lock().unwrap();
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = *self.last_failure_time.lock().unwrap() {
                    if last_failure.elapsed() >= self.timeout {
                        // Move to half-open
                        *self.state.lock().unwrap() = CircuitState::HalfOpen;
                        info!("Circuit breaker moving to half-open state");
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a success
    pub fn record_success(&self) {
        let state = *self.state.lock().unwrap();
        match state {
            CircuitState::HalfOpen => {
                // Success in half-open means we can close the circuit
                *self.state.lock().unwrap() = CircuitState::Closed;
                self.failure_count.store(0, Ordering::Relaxed);
                info!("Circuit breaker closed after successful recovery");
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::Open => {
                // Shouldn't happen, but reset anyway
                self.failure_count.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Record a failure
    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_failure_time.lock().unwrap() = Some(Instant::now());

        let state = *self.state.lock().unwrap();
        if count >= self.failure_threshold && state == CircuitState::Closed {
            *self.state.lock().unwrap() = CircuitState::Open;
            error!(
                "Circuit breaker opened after {count} failures (threshold: {threshold})",
                count = count,
                threshold = self.failure_threshold
            );
        } else if state == CircuitState::HalfOpen {
            // Failure in half-open means we go back to open
            *self.state.lock().unwrap() = CircuitState::Open;
            warn!("Circuit breaker reopened after failure in half-open state");
        }
    }

    /// Get current state
    pub fn state(&self) -> String {
        match *self.state.lock().unwrap() {
            CircuitState::Closed => "closed".to_string(),
            CircuitState::Open => "open".to_string(),
            CircuitState::HalfOpen => "half-open".to_string(),
        }
    }
}

/// Default circuit breaker for order operations
pub fn order_circuit_breaker() -> CircuitBreaker {
    CircuitBreaker::new(5, 60) // 5 failures in 60 seconds opens the circuit
}
