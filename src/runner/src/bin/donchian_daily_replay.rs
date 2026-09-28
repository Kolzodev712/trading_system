//! Deprecated entry point. Use `cargo run -p research_cli -- donchian-daily ...`.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    research_cli::donchian_daily::run_cli().await
}
