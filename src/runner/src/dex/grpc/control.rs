//! gRPC control-plane wiring for the DEX runner.

use std::sync::Arc;

use dex_engine::runtime::DexEngineRuntime;
use smol_str::SmolStr;
use tokio::sync::RwLock;
use trading_engine::dex::{DexIndicatorEngine, DexQuoteManager, DexStrategyManager};

/// Control-plane handles exposed to the CLI over gRPC (optional per engine).
pub struct DexGrpcControl {
    pub dex_runtime: Arc<DexEngineRuntime>,
    pub quote_manager: Option<Arc<RwLock<DexQuoteManager>>>,
    pub strategy_manager: Option<Arc<RwLock<DexStrategyManager>>>,
    pub balance_manager_id: Option<SmolStr>,
    pub indicator_engine: Option<Arc<DexIndicatorEngine>>,
}

impl DexGrpcControl {
    pub fn new(dex_runtime: Arc<DexEngineRuntime>) -> Self {
        Self {
            dex_runtime,
            quote_manager: None,
            strategy_manager: None,
            balance_manager_id: None,
            indicator_engine: None,
        }
    }

    pub fn with_indicators(mut self, engine: Arc<DexIndicatorEngine>) -> Self {
        self.indicator_engine = Some(engine);
        self
    }

    pub fn with_quotes(
        mut self,
        quote_manager: Arc<RwLock<DexQuoteManager>>,
        balance_manager_id: SmolStr,
    ) -> Self {
        self.quote_manager = Some(quote_manager);
        self.balance_manager_id = Some(balance_manager_id);
        self
    }

    pub fn with_strategies(mut self, strategy_manager: Arc<RwLock<DexStrategyManager>>) -> Self {
        self.strategy_manager = Some(strategy_manager);
        self
    }
}
