//! DEX levels engine wiring (DeepBook fixed quotes).

use std::collections::HashMap;
use std::sync::Arc;

use dex_engine::runtime::DexEngineRuntime;
use dex_engine::DeepBookPoolMeta;
use smol_str::SmolStr;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use trading_core::venue::dex::TradeCommand;
use trading_engine::dex::{DexLevelsEngine, DexLevelsFile, DexQuoteManager};

use crate::config::BotConfig;
use crate::SharedServices;

pub(crate) struct LevelsEngineContext {
    pub quote_manager: Arc<RwLock<DexQuoteManager>>,
    pub balance_manager_id: SmolStr,
    pub pool_meta: HashMap<SmolStr, DeepBookPoolMeta>,
}

pub(crate) struct LevelsShutdown {
    pub balance_manager_id: SmolStr,
    pub pool_meta: HashMap<SmolStr, DeepBookPoolMeta>,
}

pub(crate) fn prepare(
    bot_config: &BotConfig,
    runtime: &DexEngineRuntime,
    config_path: &str,
    indicators: Option<Arc<trading_engine::dex::DexIndicatorEngine>>,
) -> Result<LevelsEngineContext, Box<dyn std::error::Error>> {
    let dex_config = bot_config
        .dex_engine
        .as_ref()
        .ok_or("dex_engine config required for DEX levels")?;
    let deepbook = dex_config
        .deepbook_config()
        .map_err(|e| format!("invalid dex_engine config: {e}"))?;
    deepbook
        .validate_signer()
        .map_err(|e| format!("signing key: {e}"))?;

    let balance_manager_id = deepbook
        .balance_manager_id()
        .ok_or(
            "balanceManagerId required in dex_engine config for DEX levels (run create_balance_manager first)",
        )?;
    let balance_manager_id = SmolStr::new(balance_manager_id);

    let pool_meta = runtime
        .pool_meta()
        .ok_or("DEX runtime missing pool metadata (DeepBook bootstrap failed)?")?
        .as_ref()
        .clone();

    let levels_file = DexLevelsFile::load_from_file(config_path)
        .map_err(|e| format!("{e} (path: {config_path})"))?;
    info!(
        "Loaded {} DEX quote(s) from {}",
        levels_file.quotes.len(),
        config_path
    );

    let mut quote_manager =
        DexQuoteManager::from_file(levels_file, balance_manager_id.clone(), pool_meta.clone())?;
    if let Some(engine) = indicators {
        quote_manager = quote_manager.with_indicators(engine);
    }

    let quote_manager = Arc::new(RwLock::new(quote_manager));

    Ok(LevelsEngineContext {
        quote_manager,
        balance_manager_id,
        pool_meta,
    })
}

pub(crate) fn spawn(
    levels: LevelsEngineContext,
    runtime: Arc<DexEngineRuntime>,
    services: &SharedServices,
) -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    let dex_update_rx = services
        .venue
        .dex_update_rx()
        .ok_or("DEX venue update channel required")?;

    let levels_engine = DexLevelsEngine::new(levels.quote_manager, runtime, dex_update_rx);

    let shutdown_rx = services.shutdown_manager.subscribe();
    Ok(tokio::spawn(async move {
        if let Err(e) = levels_engine.run(shutdown_rx).await {
            warn!("DEX levels engine error: {e}");
        }
    }))
}

pub(crate) async fn cancel_all_on_shutdown(runtime: &DexEngineRuntime, shutdown: &LevelsShutdown) {
    for meta in shutdown.pool_meta.values() {
        match runtime
            .execute(TradeCommand::CancelAllOrders {
                pool_id: meta.pool_id.clone(),
                balance_manager_id: shutdown.balance_manager_id.clone(),
            })
            .await
        {
            Ok(result) => info!(
                pool = %meta.pool_name,
                signature = %result.signature,
                "submitted cancel-all on shutdown"
            ),
            Err(e) => warn!(pool = %meta.pool_name, "cancel-all on shutdown failed: {e}"),
        }
    }
}
