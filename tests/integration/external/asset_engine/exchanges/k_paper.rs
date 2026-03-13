//! k-paper Asset Engine External Integration Tests
//!
//! Tests GET /exchange_info against a running k-paper server.
//! All tests are #[ignore] and require the server to be running.
//!
//! Run with: cargo test --test integration integration::external::asset_engine::exchanges::k_paper -- --ignored
//!
//! Set PAPER_REST_BASE (default http://127.0.0.1:19999) if the server is elsewhere.

use asset_engine::exchanges::KPaperPublicClient;
use asset_engine::AssetService;
use std::sync::Arc;
use trading_core::types::common::{Exchange, Pair};

fn paper_rest_base() -> String {
    std::env::var("PAPER_REST_BASE").unwrap_or_else(|_| "http://127.0.0.1:19999".to_string())
}

/// Test fetching market data from k-paper GET /exchange_info
#[tokio::test]
#[ignore]
async fn test_fetch_market_data_direct() {
    let base = paper_rest_base();
    let client: Arc<dyn asset_engine::AssetDataClient> = Arc::new(KPaperPublicClient::new(base));
    let pairs = vec![Pair::new("BTC", "USDT")];

    let market_data_result = client.fetch_market_data(&pairs).await;
    assert!(
        market_data_result.is_ok(),
        "Failed to fetch market data: {:?}",
        market_data_result.err()
    );

    let market_data = market_data_result.unwrap();
    assert!(
        !market_data.symbol_info.is_empty(),
        "Market data should contain symbol info"
    );
    assert!(
        !market_data.trading_rules.is_empty(),
        "Market data should contain trading rules"
    );
}

/// Test fetching market data via AssetService (Paper registered)
#[tokio::test]
#[ignore]
async fn test_fetch_market_data_via_service() {
    let pairs = vec![Pair::new("BTC", "USDT"), Pair::new("ETH", "USDT")];
    let client: Arc<dyn asset_engine::AssetDataClient> =
        Arc::new(KPaperPublicClient::new(paper_rest_base()));
    let service = AssetService::new();

    service
        .register_exchange(Exchange::Paper, client)
        .await
        .unwrap();

    let result = service.fetch_market_data(Exchange::Paper, &pairs).await;
    assert!(
        result.is_ok(),
        "Failed to fetch market data: {:?}",
        result.err()
    );

    let market_data = result.unwrap();
    for pair in &pairs {
        assert!(
            market_data.symbol_info.contains_key(pair),
            "Missing symbol info for {:?}",
            pair
        );
        assert!(
            market_data.trading_rules.contains_key(pair),
            "Missing trading rules for {:?}",
            pair
        );
    }
}
