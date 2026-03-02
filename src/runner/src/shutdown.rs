use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Manages graceful shutdown of the trading system.
#[derive(Clone)]
pub struct ShutdownManager {
    shutdown_tx: Arc<broadcast::Sender<()>>,
}

impl ShutdownManager {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            shutdown_tx: Arc::new(tx),
        }
    }

    pub fn signal_shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Wait for shutdown signal (blocks until signal is received)
    pub async fn wait_for_shutdown(&self) {
        let mut rx = self.subscribe();
        let _ = rx.recv().await;
        info!("Shutdown signal received");
    }
}

impl Default for ShutdownManager {
    fn default() -> Self {
        Self::new(1) // Default capacity of 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn test_shutdown_manager_new() {
        let _manager = ShutdownManager::new(10);
        // Should not panic
        // Test passes if we get here
    }

    #[test]
    fn test_shutdown_manager_default() {
        let _manager = ShutdownManager::default();
        // Should not panic
        // Test passes if we get here
    }

    #[tokio::test]
    async fn test_shutdown_manager_signal_and_wait() {
        let manager = ShutdownManager::new(1);

        // Spawn a task that waits for shutdown
        let manager_clone = manager.clone();
        let wait_task = tokio::spawn(async move {
            manager_clone.wait_for_shutdown().await;
        });

        // Give it a moment to start waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Signal shutdown
        manager.signal_shutdown();

        // Wait for the task to complete (should happen quickly)
        let result = timeout(Duration::from_secs(1), wait_task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_manager_subscribe() {
        let manager = ShutdownManager::new(1);
        let mut rx = manager.subscribe();

        // Signal shutdown
        manager.signal_shutdown();

        // Should receive the signal
        let result = timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_manager_multiple_subscribers() {
        let manager = ShutdownManager::new(10);
        let mut rx1 = manager.subscribe();
        let mut rx2 = manager.subscribe();

        // Signal shutdown
        manager.signal_shutdown();

        // Both should receive the signal
        let result1 = timeout(Duration::from_millis(100), rx1.recv()).await;
        let result2 = timeout(Duration::from_millis(100), rx2.recv()).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }
}
