//! DEX strategy control RPC handlers.

use std::sync::Arc;

use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use trading_engine::dex::DexStrategyManager;

use crate::grpc::trading_cli;

pub struct DexStrategyService {
    pub strategy_manager: Arc<RwLock<DexStrategyManager>>,
}

impl DexStrategyService {
    pub async fn toggle_dex_strategy(
        &self,
        request: Request<trading_cli::ToggleDexStrategyRequest>,
    ) -> Result<Response<trading_cli::ToggleDexStrategyResponse>, Status> {
        let req = request.into_inner();
        if req.strategy_id.is_empty() {
            return Err(Status::invalid_argument("strategy_id is required"));
        }

        let mut mgr = self.strategy_manager.write().await;
        mgr.toggle_strategy(&req.strategy_id, req.enabled)
            .map_err(Status::not_found)?;

        Ok(Response::new(trading_cli::ToggleDexStrategyResponse {
            success: true,
            message: format!(
                "DEX strategy {} {}",
                req.strategy_id,
                if req.enabled { "enabled" } else { "disabled" }
            ),
        }))
    }
}
