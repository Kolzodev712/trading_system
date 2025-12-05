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
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
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

    /// Wait for shutdown signal with timeout
    #[allow(dead_code)] // Used in main.rs
    pub async fn wait_for_shutdown(&self, timeout_secs: u64) {
        let mut rx = self.subscribe();
        tokio::select! {
            _ = rx.recv() => {
                info!("Shutdown signal received");
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(timeout_secs)) => {
                info!("Shutdown timeout reached, forcing shutdown");
            }
        }
    }
}

impl Default for ShutdownManager {
    fn default() -> Self {
        Self::new()
    }
}
