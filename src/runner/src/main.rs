use runner::config::BotConfig;
use runner::mode_config::{resolve_mode_config, ModeConfigOverrides};
use runner::{initialize_mongodb, run};
use tracing::info;

use clap::Parser;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    run(bot_config, args.mode, paths, mongo_client, tracing_handles).await
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

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(log_level)
            .finish(),
    );
}

fn log_startup_info(bot_config: &BotConfig, mode: &str) {
    info!("=== Starting Trading System ===");
    info!("Mode: {}", mode);
    info!("Venue mode: {:?}", bot_config.venue.mode);
    info!("Exchange: {:?}", bot_config.exchange_engine.exchange);
    info!(
        "MongoDB persistence: {}",
        if bot_config.mongo.enabled {
            format!(
                "enabled ({}/{})",
                bot_config.mongo.connection_string.expose(),
                bot_config.mongo.database
            )
        } else {
            "disabled".to_string()
        }
    );
}
