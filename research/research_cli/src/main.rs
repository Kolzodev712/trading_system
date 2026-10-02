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
    /// Frozen Candidate #2 — BTCPriceMALongFlatV1 research screen
    #[command(name = "btc-price-ma")]
    BtcPriceMa {
        #[command(subcommand)]
        command: research_cli::btc_price_ma::Command,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Commands::BtcPriceMa { command } => research_cli::btc_price_ma::execute(command).await,
    }
}
