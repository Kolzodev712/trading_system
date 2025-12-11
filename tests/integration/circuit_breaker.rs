//! Circuit breaker functionality tests

use trading_engine::risk::circuit_breaker::CircuitBreaker;

#[tokio::test]
async fn test_circuit_breaker_lifecycle() {
    let cb = CircuitBreaker::new();

    // Initially closed
    assert_eq!(
        cb.get_state().await,
        trading_engine::risk::circuit_breaker::CircuitBreakerState::Closed
    );
    assert!(cb.is_allowed().await);

    // Open circuit
    cb.open("Test reason").await;
    assert_eq!(
        cb.get_state().await,
        trading_engine::risk::circuit_breaker::CircuitBreakerState::Open
    );
    assert!(!cb.is_allowed().await);

    // Close circuit
    cb.close().await;
    assert_eq!(
        cb.get_state().await,
        trading_engine::risk::circuit_breaker::CircuitBreakerState::Closed
    );
    assert!(cb.is_allowed().await);
}

#[tokio::test]
async fn test_circuit_breaker_failure_tracking() {
    let cb = CircuitBreaker::new();

    // Record failures
    for _ in 0..5 {
        cb.record_failure().await;
    }

    // Should be open after max failures
    assert_eq!(
        cb.get_state().await,
        trading_engine::risk::circuit_breaker::CircuitBreakerState::Open
    );
}

#[tokio::test]
async fn test_circuit_breaker_success_reset() {
    let cb = CircuitBreaker::new();

    // Record some failures
    cb.record_failure().await;
    cb.record_failure().await;

    // Record success - should reset failure count
    cb.record_success().await;

    // Should still be closed
    assert_eq!(
        cb.get_state().await,
        trading_engine::risk::circuit_breaker::CircuitBreakerState::Closed
    );
}

#[tokio::test]
async fn test_circuit_breaker_open_reason() {
    let cb = CircuitBreaker::new();
    cb.open("Test failure reason").await;

    let reason = cb.get_open_reason().await;
    assert!(reason.is_some());
    assert!(reason.unwrap().as_str().contains("Test failure reason"));
}
