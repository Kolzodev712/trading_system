//! Deposit wallet coins into a DeepBook BalanceManager.
//!
//! Requires signing key in bot config, funded wallet, and `balanceManagerId`.

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use deepbook_tools::load_deepbook_config_for_trading;
use dex_engine::chains::sui::execution::DeepBookExecutor;
use dex_engine::chains::sui::grpc::SuiGrpcClient;
use dex_engine::chains::sui::indexer::DeepBookIndexerClient;
use dex_engine::chains::sui::types::DeepBookPoolMeta;
use dex_engine::DeepBookConfig;
use rust_decimal::Decimal;
use smol_str::SmolStr;
use std::str::FromStr;
use trading_core::venue::dex::TradeCommand;

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(e) = run().await {
        if e.to_string() == "help" {
            print_help();
            return ExitCode::SUCCESS;
        }
        eprintln!("deposit_to_balance_manager failed: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

struct DepositArgs {
    config_path: std::path::PathBuf,
    sui_amount: Option<Decimal>,
    dbusdc_amount: Option<Decimal>,
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let deepbook = load_deepbook_config_for_trading(&args.config_path)?;
    let balance_manager_id = deepbook
        .balance_manager_id()
        .ok_or("balanceManagerId required in dex_engine.config")?;

    if args.sui_amount.is_none() && args.dbusdc_amount.is_none() {
        return Err("specify at least one of --sui AMOUNT or --dbusdc AMOUNT".into());
    }

    let pools = load_pools(&deepbook).await?;
    let pool = pools
        .meta
        .values()
        .next()
        .ok_or("no pools loaded from indexer")?;
    let sui_type = pool.base_asset_id.clone();
    let dbusdc_type = pool.quote_asset_id.clone();
    let executor = DeepBookExecutor::new(deepbook.clone(), pools.grpc, pools.meta).await?;

    eprintln!(
        "Depositing into BalanceManager {balance_manager_id} (signer {})",
        executor.signer_address()
    );

    let bm = SmolStr::new(balance_manager_id);
    if let Some(amount) = args.sui_amount {
        deposit(&executor, &bm, sui_type.as_str(), amount).await?;
    }
    if let Some(amount) = args.dbusdc_amount {
        deposit(&executor, &bm, dbusdc_type.as_str(), amount).await?;
    }

    println!("Deposit transaction(s) submitted successfully.");
    Ok(())
}

struct LoadedPools {
    grpc: SuiGrpcClient,
    meta: Arc<HashMap<smol_str::SmolStr, DeepBookPoolMeta>>,
}

async fn load_pools(deepbook: &DeepBookConfig) -> Result<LoadedPools, Box<dyn std::error::Error>> {
    let indexer = DeepBookIndexerClient::new(deepbook.indexer_url())?;
    let grpc = SuiGrpcClient::connect(deepbook.grpc_url())?;
    let pool_list = indexer.get_pools_by_name(&deepbook.pools).await?;
    let mut meta = HashMap::new();
    for p in pool_list {
        meta.insert(p.pool_name.clone(), p);
    }
    Ok(LoadedPools {
        grpc,
        meta: Arc::new(meta),
    })
}

async fn deposit(
    executor: &DeepBookExecutor,
    balance_manager_id: &SmolStr,
    coin_type: &str,
    amount: Decimal,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("  deposit {amount} {coin_type}");
    let result = executor
        .execute(TradeCommand::Deposit {
            balance_manager_id: balance_manager_id.clone(),
            coin_type: SmolStr::new(coin_type),
            amount,
        })
        .await?;
    println!("  tx digest: {} ({:?})", result.signature, result.status);
    Ok(())
}

fn parse_args() -> Result<DepositArgs, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut config_path = std::path::PathBuf::from("config/bot.dex.example.json");
    let mut sui_amount = None;
    let mut dbusdc_amount = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                config_path =
                    std::path::PathBuf::from(args.next().ok_or("missing --config value")?);
            }
            "--sui" => {
                sui_amount = Some(parse_decimal(
                    args.next().ok_or("missing --sui value")?.as_str(),
                )?);
            }
            "--dbusdc" => {
                dbusdc_amount = Some(parse_decimal(
                    args.next().ok_or("missing --dbusdc value")?.as_str(),
                )?);
            }
            "--help" | "-h" => return Err("help".into()),
            other => return Err(format!("unknown argument `{other}` (try --help)").into()),
        }
    }

    Ok(DepositArgs {
        config_path,
        sui_amount,
        dbusdc_amount,
    })
}

fn parse_decimal(raw: &str) -> Result<Decimal, Box<dyn std::error::Error>> {
    Decimal::from_str(raw.trim()).map_err(|e| format!("invalid decimal `{raw}`: {e}").into())
}

fn print_help() {
    eprintln!(
        "Usage: deposit_to_balance_manager [OPTIONS]

Deposit SUI and/or DBUSDC from the wallet into the configured DeepBook BalanceManager.

Environment:
  Optional fallback when privateKey is not in bot config:
  SUI_PRIVATE_KEY   (or the name set in privateKeyEnv)

Options:
  -c, --config PATH   Bot JSON with dex_engine section (default: config/bot.dex.example.json)
      --sui AMOUNT    SUI amount to deposit (human units, e.g. 5 or 1.5)
      --dbusdc AMOUNT DBUSDC amount to deposit (human units, e.g. 100)
  -h, --help          Show this help

Example:
  cargo run --release -p deepbook_tools --bin deposit_to_balance_manager -- \\
    --config config/bot.dex.example.json --sui 2 --dbusdc 50
"
    );
}
