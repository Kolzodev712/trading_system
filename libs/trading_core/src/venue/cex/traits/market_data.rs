//! Market data client trait
use crate::error::EngineError;
use crate::types::common::{Exchange, Pair};
use crate::types::futures::FundingRate;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::fmt::Debug;

/// Trait that defines the MarketDataClient interactions with the exchange
///
/// Each exchange implementation should implement the `MarketDataClient` trait to provide
/// an abstraction over the exchange market data APIs.
#[async_trait]
pub trait MarketDataClient<InstrumentId = Pair>: Debug + Send + Sync + 'static
where
    InstrumentId: Clone + Debug + Send + Sync + 'static,
{
    /// Universal identifier of the exchange this client serves
    fn exchange_id(&self) -> Exchange;

    /// Start streaming orderbooks
    async fn orderbooks(&mut self) -> Result<(), EngineError>;

    /// Start streaming trades
    async fn trades(&mut self) -> Result<(), EngineError>;

    // ===== FUTURES MARKET DATA METHODS (Optional) =====
    // All have default implementations that return "not supported"
    // Exchanges with unified APIs (like Bybit) implement these in the same client
    // Exchanges with separate APIs (like Binance) have separate clients

    /// Fetch current funding rate for a pair
    /// Default implementation returns "not supported"
    async fn fetch_funding_rate(&self, _pair: &InstrumentId) -> Result<FundingRate, EngineError> {
        Err(EngineError::NotSupported(
            "Funding rate not supported".into(),
        ))
    }

    /// Fetch mark price for a pair
    /// Default implementation returns "not supported"
    async fn fetch_mark_price(&self, _pair: &InstrumentId) -> Result<Decimal, EngineError> {
        Err(EngineError::NotSupported("Mark price not supported".into()))
    }

    /// Fetch index price for a pair
    /// Default implementation returns "not supported"
    async fn fetch_index_price(&self, _pair: &InstrumentId) -> Result<Decimal, EngineError> {
        Err(EngineError::NotSupported(
            "Index price not supported".into(),
        ))
    }

    /// Stream funding rate updates
    /// Default implementation returns "not supported"
    async fn stream_funding_rate(&mut self) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Funding rate streaming not supported".into(),
        ))
    }

    /// Stream mark price updates
    /// Default implementation returns "not supported"
    async fn stream_mark_price(&mut self) -> Result<(), EngineError> {
        Err(EngineError::NotSupported(
            "Mark price streaming not supported".into(),
        ))
    }

    /// Fetch open interest
    /// Default implementation returns "not supported"
    async fn fetch_open_interest(&self, _pair: &InstrumentId) -> Result<Decimal, EngineError> {
        Err(EngineError::NotSupported(
            "Open interest not supported".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock implementation to test default futures market data methods
    #[derive(Debug)]
    struct MockSpotOnlyMarketDataClient;

    #[async_trait]
    impl MarketDataClient for MockSpotOnlyMarketDataClient {
        fn exchange_id(&self) -> Exchange {
            Exchange::Binance
        }

        async fn orderbooks(&mut self) -> Result<(), EngineError> {
            Ok(())
        }

        async fn trades(&mut self) -> Result<(), EngineError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_default_futures_market_data_methods_return_not_supported() {
        let mut client = MockSpotOnlyMarketDataClient;
        let pair = Pair::new("BTC", "USDT");

        // Test all default futures market data methods return NotSupported
        assert!(matches!(
            client.fetch_funding_rate(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.fetch_mark_price(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.fetch_index_price(&pair).await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.stream_funding_rate().await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.stream_mark_price().await,
            Err(EngineError::NotSupported(_))
        ));

        assert!(matches!(
            client.fetch_open_interest(&pair).await,
            Err(EngineError::NotSupported(_))
        ));
    }
}
