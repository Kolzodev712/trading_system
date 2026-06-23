//! Unified DEX runner — optional levels and strategies engines in one process.

use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;
use trading_engine::dex::DexStrategyManager;

use super::bootstrap;
use super::grpc::DexGrpcControl;
use super::levels;
use super::strategies;

use crate::config::BotConfig;
use crate::SharedServices;

/// Which DEX engines to start (at least one must be set).
#[derive(Debug, Clone, Default)]
pub struct DexRunConfig {
    pub levels_config: Option<String>,
    pub strategies_config: Option<String>,
}

impl DexRunConfig {
    pub fn levels_only(path: impl Into<String>) -> Self {
        Self {
            levels_config: Some(path.into()),
            strategies_config: None,
        }
    }

    pub fn strategies_only(path: impl Into<String>) -> Self {
        Self {
            levels_config: None,
            strategies_config: Some(path.into()),
        }
    }
}

/// Run DEX mode with one or both engines (levels quotes, strategy policies).
pub async fn run(
    bot_config: BotConfig,
    config: DexRunConfig,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    if config.levels_config.is_none() && config.strategies_config.is_none() {
        return Err(
            "DEX runner requires at least one of levels_config or strategies_config".into(),
        );
    }

    let engines_label = engines_label(&config);
    info!("=== Running DEX ({engines_label}) ===");

    let runtime = services.venue.dex_runtime().ok_or("DEX runtime required")?;

    let mut grpc_control = DexGrpcControl::new(runtime.clone());
    if let Some(engine) = services.dex_indicator_engine.clone() {
        grpc_control = grpc_control.with_indicators(engine);
    }
    let mut engine_handles: Vec<JoinHandle<()>> = Vec::new();
    let mut levels_shutdown: Option<levels::LevelsShutdown> = None;
    let mut levels_ctx: Option<levels::LevelsEngineContext> = None;
    let mut strategy_manager: Option<Arc<RwLock<DexStrategyManager>>> = None;

    if let Some(ref path) = config.levels_config {
        let ctx = levels::prepare(
            &bot_config,
            &runtime,
            path,
            services.dex_indicator_engine.clone(),
        )?;
        levels_shutdown = Some(levels::LevelsShutdown {
            balance_manager_id: ctx.balance_manager_id.clone(),
            pool_meta: ctx.pool_meta.clone(),
        });
        grpc_control =
            grpc_control.with_quotes(ctx.quote_manager.clone(), ctx.balance_manager_id.clone());
        levels_ctx = Some(ctx);
    }

    if let Some(ref path) = config.strategies_config {
        let manager = strategies::prepare(path)?;
        grpc_control = grpc_control.with_strategies(manager.clone());
        strategy_manager = Some(manager);
    }

    bootstrap::start_platform(&bot_config, &services, grpc_control).await?;

    if let Some(ctx) = levels_ctx {
        engine_handles.push(levels::spawn(ctx, runtime.clone(), &services)?);
    }

    if let Some(manager) = strategy_manager {
        engine_handles.push(strategies::spawn(manager, runtime.clone(), &services)?);
    }

    services.shutdown_manager.wait_for_shutdown().await;
    info!("Shutting down DEX ({engines_label})...");

    for handle in engine_handles {
        handle.abort();
    }

    if let Some(shutdown) = levels_shutdown {
        levels::cancel_all_on_shutdown(&runtime, &shutdown).await;
    }

    Ok(())
}

fn engines_label(config: &DexRunConfig) -> String {
    match (&config.levels_config, &config.strategies_config) {
        (Some(_), Some(_)) => "levels + strategies".to_string(),
        (Some(_), None) => "levels".to_string(),
        (None, Some(_)) => "strategies".to_string(),
        (None, None) => "none".to_string(),
    }
}

/// Backward-compatible entry for `--mode dex`.
pub async fn run_levels(
    bot_config: BotConfig,
    levels_config_path: String,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    run(
        bot_config,
        DexRunConfig::levels_only(levels_config_path),
        services,
    )
    .await
}

/// Backward-compatible entry for `--mode dex-strategies`.
pub async fn run_strategies(
    bot_config: BotConfig,
    strategies_config_path: String,
    services: SharedServices,
) -> Result<(), Box<dyn std::error::Error>> {
    run(
        bot_config,
        DexRunConfig::strategies_only(strategies_config_path),
        services,
    )
    .await
}
