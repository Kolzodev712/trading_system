//! Offline strategy research CLI. Does not submit orders or load live bot config.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "research_cli",
    about = "Historical research commands. Does not submit orders."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Frozen Candidate #1 — DonchianDailyV1
    #[command(name = "donchian-daily")]
    DonchianDaily {
        #[command(subcommand)]
        command: research_cli::donchian_daily::Command,
    },
    /// Frozen Candidate #2 — BTCPriceMALongFlatV1
    #[command(name = "btc-price-ma")]
    BtcPriceMa {
        #[command(subcommand)]
        command: research_cli::btc_price_ma::Command,
    },
    /// Candidate #3 primary experiment — BTCLiquidationShockV1
    #[command(name = "btc-liquidation-shock")]
    BtcLiquidationShock {
        #[command(subcommand)]
        command: research_cli::btc_liquidation_shock::Command,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Commands::DonchianDaily { command } => research_cli::donchian_daily::execute(command).await,
        Commands::BtcPriceMa { command } => research_cli::btc_price_ma::execute(command).await,
        Commands::BtcLiquidationShock { command } => {
            research_cli::btc_liquidation_shock::execute(command).await
        }
    }
}
