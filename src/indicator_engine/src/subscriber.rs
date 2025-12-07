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
        while let Some(update) = self.receiver.recv().await {
            self.handle_update(update).await?;
        }
        Ok(())
    }

    async fn handle_update(&self, update: MarketDataUpdate) -> Result<(), SubscriberError> {
        // Process orderbook updates
        for orderbook in update.orderbooks {
            let bid_price = orderbook
                .best_bid_price()
                .ok_or_else(|| SubscriberError::MissingPrice(SmolStr::new("bid")))?;
            let ask_price = orderbook
                .best_ask_price()
                .ok_or_else(|| SubscriberError::MissingPrice(SmolStr::new("ask")))?;

            let price_point = PricePoint::from_orderbook(
                bid_price,
                ask_price,
                orderbook.total_volume(),
                orderbook.timestamp,
            );

            let pair_str = orderbook.pair.convert_to_smolstr();
            self.calculator
                .update(pair_str.as_str(), price_point)
                .map_err(SubscriberError::CalculatorError)?;
        }

        // Process trade updates
        for trade in update.trades {
            let price_point = PricePoint::from_trade(trade.price, trade.quantity, trade.timestamp);

            let pair_str = trade.pair.convert_to_smolstr();
            self.calculator
                .update(pair_str.as_str(), price_point)
                .map_err(SubscriberError::CalculatorError)?;
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
