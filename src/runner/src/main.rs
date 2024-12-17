use trading_engine::test_binance_stream;

#[tokio::main]
async fn main() {
    match test_binance_stream().await {
        Ok(shit) => println!("SHIT: {:?}", shit),
        Err(e) => eprintln!("SHIIIT: {:?}", e),
    }
}
