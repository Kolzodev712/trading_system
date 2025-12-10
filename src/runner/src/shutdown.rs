use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Manages graceful shutdown of the trading system
#[derive(Clone)]
pub struct ShutdownManager {
    #[allow(dead_code)] // Used internally and via Clone
    shutdown_tx: Arc<broadcast::Sender<()>>,
}

impl ShutdownManager {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            shutdown_tx: Arc::new(tx),
        }
    }

    #[allow(dead_code)] // Used in main.rs
    pub fn signal_shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    #[allow(dead_code)] // Used in main.rs
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Wait for shutdown signal (blocks until signal is received)
    #[allow(dead_code)] // Used in main.rs
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
