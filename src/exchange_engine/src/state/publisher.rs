//! Publisher for exchange state updates
//!
//! This module provides the `ExchangeStatePublisher` which publishes state updates
//! via broadcast channels for ultra-low latency fan-out to multiple consumers.

use tracing::{debug, warn};
use trading_core::{
    error::EngineError,
    types::exchange::{Balance, Order, Position, Trade},
    venue::cex::channels::CexVenueUpdateSender,
};
use trading_trace::{get_tracer, ColdTraceContext};

/// Publisher for exchange state updates via broadcast channels
///
/// Publishes individual updates (not batches) for ultra-low latency (< 1μs).
/// Uses broadcast channels for efficient fan-out to multiple consumers.
#[derive(Debug, Clone)]
pub struct ExchangeStatePublisher {
    /// Broadcast channel sender for exchange updates
    exchange_update_tx: CexVenueUpdateSender,
}

impl ExchangeStatePublisher {
    /// Create a new publisher with a channel sender
    pub fn new(exchange_update_tx: CexVenueUpdateSender) -> Self {
        Self { exchange_update_tx }
    }

    /// Publish an order update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_order(&self, order: &Order) -> Result<(), EngineError> {
        use trading_core::venue::cex::channels::CexVenueUpdate;

        let tracer = get_tracer("exchange-engine");
        let mut ctx = ColdTraceContext::new_root_with_tracer(&tracer, "exchange.order", None);
        ctx.set_attribute("order_id", order.order_id.as_str().to_string());
        let to_send = ctx.clone();
        ctx.end();

        self.exchange_update_tx
            .send(CexVenueUpdate::Order {
                order: order.clone(),
                trace_context: Some(to_send),
            })
            .map_err(|e| {
                warn!(
                    "Failed to publish order update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to publish order: channel closed"
                ))
            })?;

        debug!(
            "Published order update: order_id={}",
            order.order_id.as_str()
        );
        Ok(())
    }

    /// Publish a balance update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_balance(&self, balance: &Balance) -> Result<(), EngineError> {
        use trading_core::venue::cex::channels::CexVenueUpdate;

        let tracer = get_tracer("exchange-engine");
        let mut ctx = ColdTraceContext::new_root_with_tracer(&tracer, "exchange.balance", None);
        ctx.set_attribute("symbol", balance.symbol.as_ref().to_string());
        let to_send = ctx.clone();
        ctx.end();

        self.exchange_update_tx
            .send(CexVenueUpdate::Balance {
                balance: balance.clone(),
                trace_context: Some(to_send),
            })
            .map_err(|e| {
                warn!(
                    "Failed to publish balance update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to publish balance: channel closed"
                ))
            })?;

        debug!("Published balance update: symbol={}", balance.symbol);
        Ok(())
    }

    /// Publish a trade update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_trade(&self, trade: &Trade) -> Result<(), EngineError> {
        use trading_core::venue::cex::channels::CexVenueUpdate;

        let tracer = get_tracer("exchange-engine");
        let mut ctx = ColdTraceContext::new_root_with_tracer(&tracer, "exchange.trade", None);
        ctx.set_attribute("trade_id", trade.trade_id.as_str().to_string());
        let to_send = ctx.clone();
        ctx.end();

        self.exchange_update_tx
            .send(CexVenueUpdate::Trade {
                trade: trade.clone(),
                trace_context: Some(to_send),
            })
            .map_err(|e| {
                warn!(
                    "Failed to publish trade update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to publish trade: channel closed"
                ))
            })?;

        debug!("Published trade update: trade_id={}", trade.trade_id);
        Ok(())
    }

    /// Publish a position update via broadcast channel
    ///
    /// # Errors
    /// Returns an error if the channel is closed (all receivers dropped)
    pub async fn publish_position(&self, position: &Position) -> Result<(), EngineError> {
        use trading_core::venue::cex::channels::CexVenueUpdate;

        let tracer = get_tracer("exchange-engine");
        let mut ctx = ColdTraceContext::new_root_with_tracer(&tracer, "exchange.position", None);
        ctx.set_attribute(
            "pair",
            format!(
                "{}{}",
                position.instrument.pair.base, position.instrument.pair.quote
            ),
        );
        let to_send = ctx.clone();
        ctx.end();

        self.exchange_update_tx
            .send(CexVenueUpdate::Position {
                position: position.clone(),
                trace_context: Some(to_send),
            })
            .map_err(|e| {
                warn!(
                    "Failed to publish position update: channel closed or lagged ({} receivers)",
                    e
                );
                EngineError::CustomError(smol_str::format_smolstr!(
                    "Failed to publish position: channel closed"
                ))
            })?;

        debug!(
            "Published position update: pair={}",
            position.instrument.pair
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::time::Duration;
    use tokio::time::timeout;
    use trading_core::{
        types::{
            common::{OrderId, OrderStatus, Side},
            exchange::Order,
        },
        venue::cex::channels::cex_venue_update_channel,
    };

    fn create_test_order() -> Order {
        // symbol is the entire pair string (e.g., "BTCUSDT"), need to parse it
        let registry = trading_core::registry::symbol::SymbolRegistry::default();
        let pair = registry.get_or_parse_pair("BTCUSDT");
        Order {
            order_id: OrderId::new("12345".into()),
            client_order_id: None,
            instrument: trading_core::types::common::Instrument::new(
                trading_core::types::common::InstrumentType::Spot,
                pair,
            ),
            side: Side::Buy,
            price: Decimal::new(100, 0),
            amount: Decimal::new(1, 0),
            internal_created_at: 0,
            exchange_created_at: None,
            status: OrderStatus::New,
            filled_amount: Decimal::ZERO,
        }
    }

    #[tokio::test]
    async fn test_publish_order() {
        let (tx, mut rx) = cex_venue_update_channel(10);
        let publisher = ExchangeStatePublisher::new(tx);
        let order = create_test_order();

        publisher.publish_order(&order).await.unwrap();

        let update = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match update {
            trading_core::venue::cex::channels::CexVenueUpdate::Order {
                order: published_order,
                ..
            } => {
                assert_eq!(published_order.order_id, order.order_id);
            }
            _ => panic!("Expected Order update"),
        }
    }
}
