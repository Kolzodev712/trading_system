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
            MarketDataUpdate::Orderbook { orderbook, .. } => {
                // Convert pair to string for calculator (only once)
                let pair_smol = orderbook.pair.convert_to_smolstr();
                let pair_str = pair_smol.as_str();

                // Check if we have both bid and ask prices
                // Skip incomplete orderbook updates (transient condition, will be resolved on next update)
                let (bid_price, ask_price) =
                    match (orderbook.best_bid_price(), orderbook.best_ask_price()) {
                        (Some(bid), Some(ask)) => (bid, ask),
                        (None, _) => {
                            tracing::debug!(
                                "Skipping orderbook update for {}: missing bid price",
                                pair_str
                            );
                            return Ok(());
                        }
                        (_, None) => {
                            tracing::debug!(
                                "Skipping orderbook update for {}: missing ask price",
                                pair_str
                            );
                            return Ok(());
                        }
                    };

                // Calculate total volume from structured orderbook
                let total_volume = orderbook.total_volume();

                // Use timestamp from orderbook
                let timestamp = orderbook.timestamp;

                let price_point =
                    PricePoint::from_orderbook(bid_price, ask_price, total_volume, timestamp);

                // Check if we have indicators registered for this pair (not just cached values)
                // This is a diagnostic check - indicators may be registered but not ready yet
                // Only check if we have indicators, avoid expensive get_indicator_ids() call
                let has_indicators = self.calculator.has_indicators_for_pair(pair_str);
                if !has_indicators {
                    // Only check total count if we don't have indicators for this pair
                    let indicator_count = self.calculator.get_indicator_ids().len();
                    if indicator_count > 0 {
                        tracing::warn!(
                            "Market data received for {} but no indicators registered for this pair. Total indicators: {}",
                            pair_str,
                            indicator_count
                        );
                    }
                }

                // Update calculator with pair information
                match self.calculator.update(pair_str, price_point) {
                    Ok(updated_values) => {
                        if !updated_values.is_empty() {
                            tracing::debug!(
                                "Updated {} indicator(s) for {}: {} values ready",
                                updated_values.len(),
                                pair_str,
                                updated_values.len()
                            );
                        }
                        // Don't log trace for indicators accumulating data - too verbose
                    }
                    Err(e) => {
                        tracing::error!("Error updating indicators for {}: {}", pair_str, e);
                        return Err(SubscriberError::CalculatorError(e));
                    }
                }
            }
            MarketDataUpdate::Trade { trade, .. } => {
                // Convert pair to string for calculator (only once)
                let pair_smol = trade.pair.convert_to_smolstr();
                let pair_str = pair_smol.as_str();
                let price_point =
                    PricePoint::from_trade(trade.price, trade.quantity, trade.timestamp);

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
