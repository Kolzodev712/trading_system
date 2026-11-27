//! k-paper exchange driver: runs the WebSocket loop and drives ExchangeStateManager.

use crate::exchanges::k_paper::client::{
    run_websocket_loop, BalanceReadySender, KPaperConfig, KPaperWebsocketLoop,
};
use crate::state::manager::ExchangeStateManager;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::info;
use trading_core::venue::cex::traits::ConnectionState;
use trading_core::venue::cex::traits::ExchangeDriver;
use trading_core::{
    error::EngineError, types::exchange::OrderRequest, venue::cex::traits::TradingClient,
};

/// Paper exchange driver: runs k-paper WebSocket loop (exchange-only; MD from market_data_engine).
pub struct PaperDriver {
    config: KPaperConfig,
    state: Arc<RwLock<crate::exchanges::k_paper::client::KPaperState>>,
    command_rx: Option<mpsc::UnboundedReceiver<crate::exchanges::k_paper::messages::KPaperCommand>>,
    state_manager: Arc<ExchangeStateManager>,
    trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
    connection_state: Arc<RwLock<ConnectionState>>,
    /// When set, signalled when the first balance message is received (for run() to wait for snapshot).
    balance_ready: BalanceReadySender,
}

impl PaperDriver {
    /// Create a new Paper driver. Caller must pass the command_rx from KPaperClient::new.
    /// If balance_ready_tx is Some, it is signalled when the first balance message is processed.
    pub fn new(
        config: KPaperConfig,
        state: Arc<RwLock<crate::exchanges::k_paper::client::KPaperState>>,
        command_rx: mpsc::UnboundedReceiver<crate::exchanges::k_paper::messages::KPaperCommand>,
        state_manager: Arc<ExchangeStateManager>,
        trading_client: Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>>,
        balance_ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> Self {
        let balance_ready = balance_ready_tx.map(|tx| Arc::new(tokio::sync::Mutex::new(Some(tx))));
        Self {
            config,
            state,
            command_rx: Some(command_rx),
            state_manager,
            trading_client,
            connection_state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            balance_ready,
        }
    }
}

#[async_trait]
impl ExchangeDriver for PaperDriver {
    async fn run(&mut self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<(), EngineError> {
        info!("Starting k-paper exchange driver...");

        let command_rx = self.command_rx.take().ok_or_else(|| {
            EngineError::CustomError(smol_str::format_smolstr!("Paper driver already run"))
        })?;

        {
            let mut guard = self.connection_state.write().await;
            *guard = ConnectionState::Connecting;
        }

        let shutdown_for_loop = shutdown_rx.resubscribe();
        let config = self.config.clone();
        let state = self.state.clone();
        let state_manager = self.state_manager.clone();
        let balance_ready = self.balance_ready.clone();

        let connection_state = self.connection_state.clone();
        let loop_handle = tokio::spawn(async move {
            run_websocket_loop(KPaperWebsocketLoop {
                config,
                command_rx,
                state,
                market_data_tx: None,
                exchange_state_manager: state_manager,
                shutdown_rx: shutdown_for_loop,
                balance_ready,
                connection_state: Some(connection_state),
            })
            .await
        });

        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping k-paper driver");
                *self.connection_state.write().await = ConnectionState::Disconnected;
            }
            result = loop_handle => {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        *self.connection_state.write().await = ConnectionState::Failed;
                        return Err(EngineError::CustomError(smol_str::format_smolstr!(
                            "k-paper WebSocket loop error: {}", e
                        )));
                    }
                    Err(e) => {
                        *self.connection_state.write().await = ConnectionState::Failed;
                        return Err(EngineError::CustomError(smol_str::format_smolstr!(
                            "k-paper WebSocket loop task failed: {}", e
                        )));
                    }
                }
            }
        }

        info!("k-paper exchange driver stopped");
        Ok(())
    }

    fn get_trading_client(&self) -> Arc<tokio::sync::RwLock<dyn TradingClient<OrderRequest>>> {
        self.trading_client.clone()
    }

    async fn is_connection_ready(&self) -> bool {
        *self.connection_state.read().await == ConnectionState::Connected
    }

    async fn get_connection_state(&self) -> ConnectionState {
        *self.connection_state.read().await
    }
}
