use runner::config::BotConfig;
use runner::{initialize_mongodb, run};
use tracing::info;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Trading mode: "strategies" or "levels"
    #[arg(short, long, default_value = "levels")]
    mode: String,

    /// Path to strategies config file (required if mode is "strategies")
    #[arg(short, long, default_value = "config/strategies.json")]
    strategies_config: String,

    /// Path to levels config file (required if mode is "levels")
    #[arg(short, long, default_value = "config/levels.json")]
    levels_config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let bot_config = BotConfig::load_from_file("config/bot.json")?;
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

    let mongo_client = initialize_mongodb(&bot_config).await?;

    run(
        bot_config,
        args.mode,
        args.strategies_config,
        args.levels_config,
        mongo_client,
        tracing_handles,
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

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(log_level)
            .finish(),
    );
}

fn log_startup_info(bot_config: &BotConfig, mode: &str) {
    info!("=== Starting Trading System ===");
    info!("Mode: {}", mode);
    info!("Exchange: {:?}", bot_config.exchange_engine.exchange);
    info!(
        "MongoDB persistence: {}",
        if bot_config.mongo.enabled {
            format!(
                "enabled ({}/{})",
                bot_config.mongo.connection_string, bot_config.mongo.database
            )
        } else {
            "disabled".to_string()
        }
    );
}
