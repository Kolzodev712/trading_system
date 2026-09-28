//! Thin composition binary: generic runner + Candidate #2 factory, one process.

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    runner::launch(Some(Arc::new(btc_price_ma_app::BtcPriceMaFactory))).await
}
