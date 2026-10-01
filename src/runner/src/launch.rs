//! Shared CLI entry for the default runner and composition binaries.

use crate::config::BotConfig;
use crate::mode_config::{resolve_mode_config, ModeConfigOverrides};
use crate::{initialize_mongodb, run};
use clap::Parser;
use std::sync::Arc;
use tracing::info;
use trading_engine::strategies::ExtraStrategyFactory;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "Trading system runner.\n\n\
        Config layout (by mode):\n\
          levels / strategies  → config/cex/bot.json (+ levels.json or strategies.json)\n\
          dex                    → config/dex/bot.json + config/dex/levels.json\n\
          dex-strategies         → config/dex/bot.json + config/dex/strategies.json\n\n\
        Override paths with --config, --levels-config, --strategies-config, \
        --dex-levels-config, or --dex-strategies-config."
)]
struct Args {
    /// Trading mode: levels, strategies, dex, or dex-strategies
    #[arg(short, long, default_value = "levels")]
    mode: String,

    /// Override bot config (default: config/cex/bot.json or config/dex/bot.json by mode)
    #[arg(short, long)]
    config: Option<String>,

    /// Override CEX strategies config (default: config/cex/strategies.json)
    #[arg(short, long)]
    strategies_config: Option<String>,

    /// Override CEX levels config (default: config/cex/levels.json)
    #[arg(short, long)]
    levels_config: Option<String>,

    /// Override DEX quotes config (default: config/dex/levels.json)
    #[arg(long)]
    dex_levels_config: Option<String>,

    /// Override DEX strategies config (default: config/dex/strategies.json)
    #[arg(long)]
    dex_strategies_config: Option<String>,
}

pub async fn launch(
    extra_factory: Option<Arc<dyn ExtraStrategyFactory>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let paths = resolve_mode_config(
        &args.mode,
        ModeConfigOverrides {
            bot: args.config,
            levels: args.levels_config,
            strategies: args.strategies_config,
            dex_levels: args.dex_levels_config,
            dex_strategies: args.dex_strategies_config,
            dex_indicators: None,
        },
    )?;

    info!(
        mode = %args.mode,
        bot = %paths.bot,
        levels = ?paths.levels,
        strategies = ?paths.strategies,
        dex_levels = ?paths.dex_levels,
        dex_strategies = ?paths.dex_strategies,
        "Using config paths"
    );

    let bot_config = BotConfig::load_from_file(&paths.bot)?;
    bot_config.validate()?;

    initialize_logging(&bot_config);

    let mut exporters = trading_trace::TraceConfig::default().exporters;
    if let Ok(endpoint) = std::env::var("TRACE_JAEGER_AGENT_ENDPOINT") {
        if let Some(ref mut jaeger) = exporters.jaeger {
            jaeger.agent_endpoint = endpoint;
        }
    }
    // opentelemetry-jaeger 0.20 panics on current tokio when installing the UDP agent
    // (`tokio_allow_from_blocking_fd`). Lab scripts set TRACE_JAEGER_DISABLED=1.
    if std::env::var("TRACE_JAEGER_DISABLED").ok().as_deref() == Some("1") {
        if let Some(ref mut jaeger) = exporters.jaeger {
            jaeger.enabled = false;
        }
    }
    let tracing_handles = match trading_trace::init_exporters(
        &exporters,
        "trading-system",
        Some(env!("CARGO_PKG_VERSION")),
    ) {
        Ok(h) => {
            trading_trace::record_startup_span();
            Some(h)
        }
        Err(e) => {
            tracing::warn!("Tracing exporters init failed (continuing without): {}", e);
            None
        }
    };

    log_startup_info(&bot_config, &args.mode);

    let mongo_client = initialize_mongodb(&bot_config, &args.mode).await?;

    run(
        bot_config,
        args.mode,
        paths,
        mongo_client,
        tracing_handles,
        extra_factory,
    )
    .await
}

fn initialize_logging(bot_config: &BotConfig) {
    let log_level = match bot_config.system.log_level.as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    let json = std::env::var("TRADING_LOG_JSON").ok().as_deref() == Some("1");
    if json {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt::Subscriber::builder()
                .with_max_level(log_level)
                .json()
                .flatten_event(true)
                .finish(),
        );
    } else {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt::Subscriber::builder()
                .with_max_level(log_level)
                .finish(),
        );
    }
}

fn log_startup_info(bot_config: &BotConfig, mode: &str) {
    let run_id = std::env::var("TRADING_RUN_ID").unwrap_or_default();
    info!(run_id = %run_id, "=== Starting Trading System ===");
    info!("Mode: {}", mode);
    info!("Venue mode: {:?}", bot_config.venue.mode);
    info!("Exchange: {:?}", bot_config.exchange_engine.exchange);
    info!(
        "MongoDB persistence: {}",
        if bot_config.mongo.enabled {
            format!("enabled (database={})", bot_config.mongo.database)
        } else {
            "disabled".to_string()
        }
    );
}
