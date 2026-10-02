//! Bybit Demo read-only comparison. Never Resume or place orders.

use trading_report::config::VenueSourceConfig;
use trading_report::venue_source::HttpVenueReader;

#[tokio::test]
async fn bybit_demo_readonly_if_credentials_present() {
    let key = std::env::var("BYBIT_DEMO_API_KEY").unwrap_or_default();
    let secret = std::env::var("BYBIT_DEMO_API_SECRET").unwrap_or_default();
    if key.is_empty() || key.contains("REPLACE") || secret.is_empty() {
        eprintln!(
            "BYBIT_DEMO_API_KEY/SECRET not configured; Paper-only scope. Demo read-only not run."
        );
        return;
    }
    let cfg = VenueSourceConfig {
        kind: "bybit_demo".into(),
        rest_base: "https://api-demo.bybit.com".into(),
        api_key_env: Some("BYBIT_DEMO_API_KEY".into()),
        api_secret_env: Some("BYBIT_DEMO_API_SECRET".into()),
        category: Some("linear".into()),
    };
    let snap = HttpVenueReader::new(cfg)
        .expect("reader")
        .snapshot("BTCUSDT", "futures")
        .await;
    assert!(
        snap.http_methods.iter().all(|m| m == "GET"),
        "Demo path must stay GET-only"
    );
    assert!(
        snap.orders.is_ok(),
        "signed Demo open orders: {:?}",
        snap.orders.err()
    );
    assert!(
        snap.positions.is_ok(),
        "signed Demo positions: {:?}",
        snap.positions.err()
    );
}
