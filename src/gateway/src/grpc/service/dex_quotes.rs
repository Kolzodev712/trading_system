//! DEX quote control RPC handlers (toggle, cancel-all).

use std::sync::Arc;

use dex_engine::runtime::DexEngineRuntime;
use smol_str::SmolStr;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::warn;
use trading_core::venue::dex::TradeCommand;
use trading_engine::dex::DexQuoteManager;

use crate::grpc::trading_cli;

pub struct DexQuoteService {
    pub quote_manager: Arc<RwLock<DexQuoteManager>>,
    pub dex_runtime: Arc<DexEngineRuntime>,
    pub balance_manager_id: SmolStr,
}

impl DexQuoteService {
    pub async fn toggle_dex_quote(
        &self,
        request: Request<trading_cli::ToggleDexQuoteRequest>,
    ) -> Result<Response<trading_cli::ToggleDexQuoteResponse>, Status> {
        let req = request.into_inner();
        if req.quote_id.is_empty() {
            return Err(Status::invalid_argument("quote_id is required"));
        }

        let mut mgr = self.quote_manager.write().await;
        mgr.toggle_quote(&req.quote_id, req.enabled)
            .map_err(Status::not_found)?;

        Ok(Response::new(trading_cli::ToggleDexQuoteResponse {
            success: true,
            message: format!(
                "Quote {} {}",
                req.quote_id,
                if req.enabled { "enabled" } else { "disabled" }
            ),
        }))
    }

    pub async fn cancel_dex_pool_orders(
        &self,
        request: Request<trading_cli::CancelDexPoolOrdersRequest>,
    ) -> Result<Response<trading_cli::CancelDexPoolOrdersResponse>, Status> {
        let req = request.into_inner();
        let pools: Vec<(SmolStr, SmolStr)> = {
            let mgr = self.quote_manager.read().await;
            if req.pool.is_empty() {
                mgr.configured_pools()
            } else {
                let pool_id = mgr.pool_id_for_name(&req.pool).ok_or_else(|| {
                    Status::invalid_argument(format!("unknown pool {}", req.pool))
                })?;
                vec![(SmolStr::new(&req.pool), pool_id)]
            }
        };

        if pools.is_empty() {
            return Ok(Response::new(trading_cli::CancelDexPoolOrdersResponse {
                success: true,
                message: "No pools configured".to_string(),
                transaction_signatures: vec![],
            }));
        }

        let mut signatures = Vec::new();
        let mut errors = Vec::new();
        for (pool_name, pool_id) in pools {
            match self
                .dex_runtime
                .execute(TradeCommand::CancelAllOrders {
                    pool_id,
                    balance_manager_id: self.balance_manager_id.clone(),
                })
                .await
            {
                Ok(result) => {
                    signatures.push(result.signature.to_string());
                }
                Err(e) => {
                    warn!(pool = %pool_name, "cancel-all failed: {e}");
                    errors.push(format!("{pool_name}: {e}"));
                }
            }
        }

        if signatures.is_empty() && !errors.is_empty() {
            return Err(Status::internal(errors.join("; ")));
        }

        let pool_label = if req.pool.is_empty() {
            "all pools".to_string()
        } else {
            req.pool.clone()
        };
        Ok(Response::new(trading_cli::CancelDexPoolOrdersResponse {
            success: true,
            message: format!(
                "Submitted cancel-all for {pool_label} ({} tx)",
                signatures.len()
            ),
            transaction_signatures: signatures,
        }))
    }
}
