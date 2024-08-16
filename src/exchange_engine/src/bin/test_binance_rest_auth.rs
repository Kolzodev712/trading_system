use exchange_engine::exchanges::binance::rest::{
    binance_rest::BinanceExchangeHandler,
    types::{AccountInfo, BinancePrivateEndpoint},
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("BINANCE_API_KEY")?;
    let secret_key = env::var("BINANCE_SECRET_KEY")?;

    let account_info_handler = BinanceExchangeHandler::new_private(
        BinancePrivateEndpoint::AccountInfo,
        None,
        None,
        api_key.clone(),
        secret_key.clone(),
    );

    let account_info: AccountInfo = account_info_handler.get().await?;

    println!("{:?}", account_info);

    Ok(())
}
