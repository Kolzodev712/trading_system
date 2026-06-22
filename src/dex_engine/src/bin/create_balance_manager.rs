//! Create and register a DeepBook V3 BalanceManager on Sui testnet/mainnet.
//!
//! Requires `SUI_PRIVATE_KEY` (or `privateKeyEnv` from config) and funded SUI for gas.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use dex_engine::config::DexEngineConfig;
use dex_engine::DeepBookConfig;
use dex_engine::DeepBookSetup;
use trading_core::venue::VenueId;

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(e) = run().await {
        eprintln!("create_balance_manager failed: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = parse_config_path()?;
    let bot: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    let dex_engine: DexEngineConfig = serde_json::from_value(
        bot.get("dex_engine")
            .cloned()
            .ok_or("config missing dex_engine section")?,
    )?;
    if dex_engine.venue != VenueId::DeepBook {
        return Err("dex_engine.venue must be deepbook".into());
    }
    let deepbook = DeepBookConfig::from_dex_engine(&dex_engine)?;

    let setup = DeepBookSetup::from_config(deepbook.clone()).await?;
    let wallet = setup.signer_address();
    eprintln!("Signing as {wallet} (network: {:?})", deepbook.network);
    let manager_id = setup.create_and_register_balance_manager().await?;

    println!("DeepBook BalanceManager created successfully.\n");
    println!("walletAddress:     {wallet}");
    println!("balanceManagerId:  {manager_id}");
    println!();
    println!("Add to dex_engine.config in your bot JSON:");
    println!(r#"  "walletAddress": "{wallet}","#);
    println!(r#"  "balanceManagerId": "{manager_id}""#);
    Ok(())
}

fn parse_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut path = PathBuf::from("config/bot.dex.example.json");
    while let Some(arg) = args.next() {
        if arg == "--config" || arg == "-c" {
            path = PathBuf::from(args.next().ok_or("missing value for --config")?);
        } else if arg == "--help" || arg == "-h" {
            print_help();
            std::process::exit(0);
        } else {
            return Err(format!("unknown argument `{arg}` (try --help)").into());
        }
    }
    Ok(path)
}

fn print_help() {
    eprintln!(
        "Usage: create_balance_manager [--config PATH]

Creates, registers, and shares a DeepBook BalanceManager on Sui.

Environment:
  SUI_PRIVATE_KEY   Signing key (suiprivkey, 0x-hex, or 64-char hex)

Options:
  -c, --config PATH Bot JSON with dex_engine section (default: config/bot.dex.example.json)
  -h, --help        Show this help
"
    );
}
