//! Deprecated entry point. Use `cargo run -p research_cli -- btc-price-ma ...`.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    research_cli::btc_price_ma::run_cli().await
}
