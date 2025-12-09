use crate::calculator::IndicatorCalculator;
use crate::data::PricePoint;
use smol_str::SmolStr;
use std::sync::Arc;
use thiserror::Error;
use trading_core::channels::market_data::{MarketDataUpdate, MarketDataUpdateReceiver};

pub struct MarketDataSubscriber {
    receiver: MarketDataUpdateReceiver,
    calculator: Arc<IndicatorCalculator>,
}

impl MarketDataSubscriber {
    pub fn new(receiver: MarketDataUpdateReceiver, calculator: Arc<IndicatorCalculator>) -> Self {
        Self {
            receiver,
            calculator,
        }
    }

    /// Start listening for market data updates
    pub async fn run(&mut self) -> Result<(), SubscriberError> {
        loop {
            match self.receiver.recv().await {
                Ok(update) => {
                    if let Err(e) = self.handle_update(update).await {
                        tracing::error!("Error handling market data update: {}", e);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        "Market data receiver lagged, skipped {skipped} messages. This may indicate slow processing."
                    );
                    // Continue receiving - broadcast channels automatically skip lagged messages
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::warn!("Market data channel closed - shutting down subscriber");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_update(&self, update: MarketDataUpdate) -> Result<(), SubscriberError> {
        match update {
            MarketDataUpdate::Orderbook { pair, orderbook } => {
                // Convert pair to string for calculator
                let pair_smol = pair.convert_to_smolstr();
                let pair_str = pair_smol.as_str();

                // LegacyOrderbook doesn't have best_bid_price/best_ask_price methods
                // We need to extract prices from the bids/asks vectors
                let bid_price = orderbook
                    .bids
                    .get(0)
                    .copied()
                    .ok_or_else(|| SubscriberError::MissingPrice(SmolStr::new("bid")))?;
                let ask_price = orderbook
                    .asks
                    .get(0)
                    .copied()
                    .ok_or_else(|| SubscriberError::MissingPrice(SmolStr::new("ask")))?;

                // Calculate total volume
                let total_volume: rust_decimal::Decimal = orderbook
                    .bids
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .chain(orderbook.asks.iter().skip(1).step_by(2))
                    .sum();

                // Use current timestamp since LegacyOrderbook doesn't have timestamp field
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let price_point =
                    PricePoint::from_orderbook(bid_price, ask_price, total_volume, timestamp);

                // Update calculator with pair information
                self.calculator
                    .update(pair_str, price_point)
                    .map_err(SubscriberError::CalculatorError)?;
            }
            MarketDataUpdate::Trade { pair, trade } => {
                // Convert pair to string for calculator
                let pair_smol = pair.convert_to_smolstr();
                let pair_str = pair_smol.as_str();
                let price_point = PricePoint::from_trade(trade.price, trade.quantity, trade.time);

                // Update calculator with pair information
                self.calculator
                    .update(pair_str, price_point)
                    .map_err(SubscriberError::CalculatorError)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SubscriberError {
    #[error("Channel error: {0}")]
    ChannelError(SmolStr),

    #[error("Missing price data: {0}")]
    MissingPrice(SmolStr),

    #[error("Calculator error: {0}")]
    CalculatorError(#[from] crate::indicators::traits::IndicatorError),
}
