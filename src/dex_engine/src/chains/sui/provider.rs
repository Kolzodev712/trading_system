//! `VenueProvider` for DeepBook V3 on Sui (read + write paths).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use smol_str::SmolStr;
use tokio::sync::broadcast;
use tokio::sync::OnceCell;
use tracing::{info, warn};
use trading_core::venue::dex::channels::DexVenueUpdate;
use trading_core::venue::dex::provider::VenueProvider;
use trading_core::venue::dex::types::{
    DexAccountSnapshot, DexOpenOrder, ExecutionResult, TxStatus,
};
use trading_core::venue::dex::{DexError, TradeCommand};
use trading_core::venue::VenueId;

use super::config::DeepBookConfig;
use super::execution::DeepBookExecutor;
use super::grpc::SuiGrpcClient;
use super::indexer::DeepBookIndexerClient;
use super::types::DeepBookPoolMeta;

pub struct DeepBookVenueProvider {
    config: DeepBookConfig,
    indexer: DeepBookIndexerClient,
    grpc: SuiGrpcClient,
    pools: Arc<HashMap<SmolStr, DeepBookPoolMeta>>,
    executor: OnceCell<DeepBookExecutor>,
    pending: Arc<parking_lot::RwLock<HashMap<smol_str::SmolStr, TradeCommand>>>,
}

impl DeepBookVenueProvider {
    pub async fn new(config: DeepBookConfig) -> Result<Self, DexError> {
        config.validate()?;
        let indexer = DeepBookIndexerClient::new(config.indexer_url())?;
        let grpc = SuiGrpcClient::connect(config.grpc_url())?;
        let pool_list = indexer.get_pools_by_name(&config.pools).await?;
        let mut pools = HashMap::new();
        for meta in pool_list {
            info!(
                "DeepBook pool loaded (chain Sui): {} (tick={}, lot={}, min={})",
                meta.pool_name, meta.tick_size, meta.lot_size, meta.min_size
            );
            pools.insert(meta.pool_name.clone(), meta);
        }
        info!("DeepBook package id: {}", config.deepbook_package_id()?);
        Ok(Self {
            config,
            indexer,
            grpc,
            pools: Arc::new(pools),
            executor: OnceCell::new(),
            pending: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }

    pub fn pool_meta(&self) -> HashMap<SmolStr, DeepBookPoolMeta> {
        self.pools.as_ref().clone()
    }

    pub fn config(&self) -> &DeepBookConfig {
        &self.config
    }

    pub fn indexer(&self) -> &DeepBookIndexerClient {
        &self.indexer
    }

    async fn executor(&self) -> Result<&DeepBookExecutor, DexError> {
        self.executor
            .get_or_try_init(|| async {
                DeepBookExecutor::new(self.config.clone(), self.grpc.clone(), self.pools.clone())
                    .await
            })
            .await
    }

    async fn fetch_account_snapshot(&self) -> Result<DexAccountSnapshot, DexError> {
        let wallet = self.fetch_wallet_balances().await?;
        let open_orders = self.fetch_open_orders().await?;
        Ok(DexAccountSnapshot {
            wallet,
            open_orders,
        })
    }

    async fn fetch_wallet_balances(
        &self,
    ) -> Result<trading_core::venue::dex::types::DexWalletBalances, DexError> {
        let Some(address) = self.config.wallet_address() else {
            return Ok(trading_core::venue::dex::types::DexWalletBalances::default());
        };
        self.grpc.list_balances(address).await
    }

    async fn fetch_open_orders(&self) -> Result<Vec<DexOpenOrder>, DexError> {
        let Some(balance_manager_id) = self.config.balance_manager_id() else {
            return Ok(Vec::new());
        };
        let mut orders = Vec::new();
        for pool_name in &self.config.pools {
            let raw = self
                .indexer
                .get_open_orders(pool_name.as_str(), balance_manager_id)
                .await?;
            for entry in raw {
                orders.push(entry.to_dex_open_order(pool_name.as_str())?);
            }
        }
        Ok(orders)
    }

    async fn poll_pending_transactions(
        &self,
        tx: &broadcast::Sender<DexVenueUpdate>,
    ) -> Result<(), DexError> {
        let digests: Vec<smol_str::SmolStr> = self.pending.read().keys().cloned().collect();
        if digests.is_empty() {
            return Ok(());
        }
        let executor = self.executor().await?;
        for digest in digests {
            let status = match executor.poll_transaction(digest.as_str()).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("pending tx poll failed for {digest}: {e}");
                    continue;
                }
            };
            if status == TxStatus::Pending {
                continue;
            }
            let command = self.pending.write().remove(&digest);
            let result = ExecutionResult {
                signature: digest.clone(),
                status,
                error: if status == TxStatus::Failed {
                    Some("transaction failed on-chain".into())
                } else {
                    None
                },
            };
            let _ = tx.send(DexVenueUpdate::ExecutionResult {
                result: result.clone(),
                trace_context: None,
            });
            if let Some(cmd) = command {
                info!(
                    "DeepBook tx {digest} {:?} (command {:?})",
                    status,
                    cmd_kind(&cmd)
                );
            }
        }
        Ok(())
    }
}

fn cmd_kind(command: &TradeCommand) -> &'static str {
    match command {
        TradeCommand::PlaceLimitOrder { .. } => "PlaceLimitOrder",
        TradeCommand::CancelOrder { .. } => "CancelOrder",
        TradeCommand::CancelAllOrders { .. } => "CancelAllOrders",
        TradeCommand::Deposit { .. } => "Deposit",
        TradeCommand::Withdraw { .. } => "Withdraw",
        TradeCommand::Liquidate { .. } => "Liquidate",
        TradeCommand::Swap { .. } => "Swap",
    }
}

#[async_trait]
impl VenueProvider for DeepBookVenueProvider {
    fn venue_id(&self) -> VenueId {
        VenueId::DeepBook
    }

    async fn bootstrap_state(&self) -> Result<DexAccountSnapshot, DexError> {
        let snapshot = self.fetch_account_snapshot().await?;
        if snapshot.wallet.balances.is_empty() && self.config.wallet_address().is_none() {
            info!("DeepBook: no walletAddress configured — skipping Sui gRPC balance read");
        } else if !snapshot.wallet.balances.is_empty() {
            info!(
                "DeepBook: loaded {} wallet balance(s) from Sui gRPC",
                snapshot.wallet.balances.len()
            );
        }
        if snapshot.open_orders.is_empty() {
            if self.config.balance_manager_id().is_none() {
                info!("DeepBook: no balanceManagerId configured — skipping indexer open orders");
            }
        } else {
            info!(
                "DeepBook: loaded {} open order(s) from indexer",
                snapshot.open_orders.len()
            );
        }
        Ok(snapshot)
    }

    async fn stream_updates(&self, tx: broadcast::Sender<DexVenueUpdate>) -> Result<(), DexError> {
        if self.config.wallet_address().is_none() && self.config.balance_manager_id().is_none() {
            info!("DeepBook stream_updates: no walletAddress or balanceManagerId — idle");
            return Ok(());
        }
        let interval = Duration::from_millis(self.config.balance_poll_interval_ms);
        info!(
            "DeepBook stream_updates: polling account snapshot every {:?}",
            interval
        );
        loop {
            if let Err(e) = self.poll_pending_transactions(&tx).await {
                warn!("DeepBook pending tx poll failed: {e}");
            }
            match self.fetch_account_snapshot().await {
                Ok(snapshot) => {
                    let _ = tx.send(DexVenueUpdate::WalletBalance {
                        balances: snapshot.wallet,
                        trace_context: None,
                    });
                    let _ = tx.send(DexVenueUpdate::OpenOrders {
                        orders: snapshot.open_orders,
                        trace_context: None,
                    });
                }
                Err(e) => warn!("DeepBook account snapshot poll failed: {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn execute(&self, command: TradeCommand) -> Result<ExecutionResult, DexError> {
        let executor = self.executor().await?;
        let result = executor.execute(command.clone()).await?;
        self.pending
            .write()
            .insert(result.signature.clone(), command);
        Ok(result)
    }

    async fn reconcile(&self) -> Result<DexAccountSnapshot, DexError> {
        self.fetch_account_snapshot().await
    }
}
