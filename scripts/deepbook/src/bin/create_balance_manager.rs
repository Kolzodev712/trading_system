//! Create and register a DeepBook V3 BalanceManager on Sui testnet/mainnet.
//!
//! Requires `privateKey` in bot config (or `privateKeyEnv` + env fallback) and funded SUI for gas.

use std::process::ExitCode;

use deepbook_tools::load_deepbook_config;
use dex_engine::DeepBookSetup;

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(e) = run().await {
        eprintln!("create_balance_manager failed: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = match deepbook_tools::parse_config_path("config/bot.dex.example.json") {
        Ok(path) => path,
        Err(e) if e.to_string() == "help" => {
            print_help();
            std::process::exit(0);
        }
        Err(e) => return Err(e),
    };
    let deepbook = load_deepbook_config(&config_path)?;

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

fn print_help() {
    eprintln!(
        "Usage: create_balance_manager [--config PATH]

Creates, registers, and shares a DeepBook BalanceManager on Sui.

Environment:
  Optional fallback when privateKey is not in bot config:
  SUI_PRIVATE_KEY   Signing key (suiprivkey, 0x-hex, or 64-char hex)

Options:
  -c, --config PATH Bot JSON with dex_engine section (default: config/bot.dex.example.json)
  -h, --help        Show this help
"
    );
}
