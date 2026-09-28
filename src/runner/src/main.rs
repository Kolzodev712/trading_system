//! Default runner binary. Candidate #2 requires `btc_price_ma_app`.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    runner::launch(None).await
}
