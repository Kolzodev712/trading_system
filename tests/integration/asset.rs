//! Asset engine tests

use asset_engine::exchanges::binance::public_client::BinancePublicClient;
use asset_engine::AssetService;
use std::sync::Arc;

#[tokio::test]
async fn test_asset_service_creation() {
    let _service = AssetService::new();
    // Should not panic
    assert!(true);
}

#[tokio::test]
async fn test_asset_service_exchange_registration() {
    let service = AssetService::new();
    // register_exchange takes exchange and client
    let client = Arc::new(BinancePublicClient::new(
        "https://api.binance.com".to_string(),
    ));
    service
        .register_exchange(trading_core::types::common::Exchange::Binance, client)
        .await
        .unwrap();
    // Should not panic
    assert!(true);
}
