use crate::state::{Strategy, TradingState};
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use trading_core::{
    error::EngineError,
    traits::TradingClient,
    types::{
        common::{OrderStatus, OrderType, Pair, Side, Symbol},
        exchange::{Order, OrderRequest},
        market_data::Orderbook,
    },
};

/// Simple "Buy Low, Sell High" strategy using mean reversion
///
/// Strategy logic:
/// - Tracks a simple moving average of recent prices
/// - Buys when price drops significantly below the average
/// - Sells when price rises significantly above the average
/// - Maintains a maximum position size
/// - Limits number of active orders
pub struct BuyLowSellHigh {
    state: TradingState,
    #[allow(dead_code)] // Reserved for future use
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    symbol: Pair,

    // Price history for moving average
    price_history: Vec<Decimal>,
    max_history: usize,
    min_price_history: usize, // Minimum price history required before trading

    // Strategy parameters
    buy_threshold: Decimal,               // Buy when price is X% below average
    sell_threshold: Decimal,              // Sell when price is X% above average
    position_size: Decimal,               // Fixed position size per trade
    max_position: Decimal,                // Maximum total position size
    max_active_orders: usize,             // Maximum concurrent orders
    stop_loss_percent: Option<Decimal>,   // Stop loss percentage (e.g., 5.0 for 5%)
    take_profit_percent: Option<Decimal>, // Take profit percentage (e.g., 10.0 for 10%)
}

impl BuyLowSellHigh {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
        symbol: Pair,
        position_size: Decimal,
        max_position: Decimal,
        buy_threshold: Decimal,
        sell_threshold: Decimal,
        stop_loss_percent: Option<Decimal>,
        take_profit_percent: Option<Decimal>,
        max_active_orders: usize,
        price_history_size: usize,
        min_price_history: usize,
    ) -> Self {
        Self {
            state: TradingState::new(),
            trading_client,
            symbol,
            price_history: Vec::new(),
            max_history: price_history_size,
            min_price_history,
            buy_threshold,
            sell_threshold,
            position_size,
            max_position,
            max_active_orders,
            stop_loss_percent,
            take_profit_percent,
        }
    }

    fn calculate_average_price(&self) -> Option<Decimal> {
        if self.price_history.is_empty() {
            return None;
        }
        let sum: Decimal = self.price_history.iter().sum();
        Some(sum / Decimal::from(self.price_history.len()))
    }

    fn get_mid_price(&self, orderbook: &Orderbook) -> Option<Decimal> {
        orderbook.mid_price()
    }

    /// Get the last known price from price history (for reconciliation)
    pub fn get_last_price(&self) -> Option<Decimal> {
        self.price_history.last().copied()
    }

    fn should_buy(&self, current_price: Decimal, avg_price: Decimal) -> bool {
        if avg_price <= Decimal::ZERO {
            return false;
        }
        // Calculate how much price is below average (positive value means price is lower)
        let price_diff = (avg_price - current_price) / avg_price * Decimal::from(100);
        price_diff >= self.buy_threshold
    }

    fn should_sell(&self, current_price: Decimal, avg_price: Decimal) -> bool {
        if avg_price <= Decimal::ZERO {
            return false;
        }
        let price_diff = (current_price - avg_price) / avg_price * Decimal::from(100);
        price_diff >= self.sell_threshold
    }
}

#[async_trait]
impl Strategy for BuyLowSellHigh {
    async fn on_orderbook(
        &mut self,
        orderbook: &Orderbook,
        state: &TradingState,
    ) -> Result<Option<OrderRequest>, EngineError> {
        // Get current mid price
        let mid_price = match self.get_mid_price(orderbook) {
            Some(p) => p,
            None => return Ok(None),
        };

        // Update price history
        self.price_history.push(mid_price);
        if self.price_history.len() > self.max_history {
            self.price_history.remove(0);
        }

        // Need minimum price history to calculate meaningful average
        let min_history = self.min_price_history;
        if self.price_history.len() < min_history {
            // Only log at key milestones to reduce noise
            if self.price_history.len() == min_history / 2
                || self.price_history.len() == min_history - 1
                || self.price_history.len() == min_history - 5
            {
                debug!(
                    "Building price history: {current}/{target} (need {remaining} more before trading)",
                    current = self.price_history.len(),
                    target = min_history,
                    remaining = min_history - self.price_history.len()
                );
            }
            return Ok(None);
        }

        let avg_price = match self.calculate_average_price() {
            Some(p) if p > Decimal::ZERO => p,
            Some(_) => {
                warn!("Average price is zero, skipping trade decision");
                return Ok(None);
            }
            None => return Ok(None),
        };

        // Check active orders limit
        let active_count = state.get_active_order_count(&self.symbol);
        if active_count >= self.max_active_orders {
            return Ok(None);
        }

        // Check current position
        let current_position = state.get_position(&self.symbol);
        let current_qty = current_position
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);

        // Check stop-loss and take-profit for existing positions
        if let Some(position) = current_position {
            if position.quantity > Decimal::ZERO {
                // Check stop-loss
                if let Some(stop_loss) = self.stop_loss_percent {
                    if position.avg_price > Decimal::ZERO {
                        let loss_percent = (position.avg_price - mid_price) / position.avg_price
                            * Decimal::from(100);
                        if loss_percent >= stop_loss {
                            warn!(
                            "Stop-loss triggered: loss={loss_percent:.2}%, threshold={stop_loss}%"
                        );
                            let sell_price = orderbook.best_bid_price().unwrap_or(mid_price);
                            return Ok(Some(OrderRequest {
                                symbol: Symbol::from(self.symbol.convert_to_smolstr()),
                                side: Side::Sell,
                                order_type: OrderType::Limit,
                                price: sell_price,
                                amount: position.quantity,
                                timestamp: None,
                                exchange_params: Default::default(),
                            }));
                        }
                    }
                }

                // Check take-profit
                if let Some(take_profit) = self.take_profit_percent {
                    if position.avg_price > Decimal::ZERO {
                        let profit_percent = (mid_price - position.avg_price) / position.avg_price
                            * Decimal::from(100);
                        if profit_percent >= take_profit {
                            info!("Take-profit triggered: profit={profit_percent:.2}%, threshold={take_profit}%");
                            let sell_price = orderbook.best_bid_price().unwrap_or(mid_price);
                            return Ok(Some(OrderRequest {
                                symbol: Symbol::from(self.symbol.convert_to_smolstr()),
                                side: Side::Sell,
                                order_type: OrderType::Limit,
                                price: sell_price,
                                amount: position.quantity,
                                timestamp: None,
                                exchange_params: Default::default(),
                            }));
                        }
                    }
                }
            }
        }

        // Decision logic
        // Log price status periodically to show what's happening
        // Guard against division by zero
        let price_diff_from_avg = if avg_price > Decimal::ZERO {
            (mid_price - avg_price) / avg_price * Decimal::from(100)
        } else {
            debug!("Average price is zero, cannot calculate price difference");
            return Ok(None);
        };

        // Periodic price status (every 30 updates to reduce noise)
        #[allow(clippy::manual_is_multiple_of)]
        if self.price_history.len() % 30 == 0 {
            debug!(
                "Price status: mid={mid_price}, avg={avg_price}, diff={diff:.3}%, position={position} BTC",
                mid_price = mid_price,
                avg_price = avg_price,
                diff = price_diff_from_avg,
                position = current_qty
            );
        }

        if self.should_buy(mid_price, avg_price) {
            // Check if we can buy more
            if current_qty + self.position_size <= self.max_position {
                let diff = if avg_price > Decimal::ZERO {
                    (avg_price - mid_price) / avg_price * Decimal::from(100)
                } else {
                    Decimal::ZERO
                };
                debug!("BUY signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%, amount={amount} BTC",
                    amount = self.position_size
                );

                // Use best ask as buy price (market order would be better, but limit is safer)
                let buy_price = orderbook.best_ask_price().unwrap_or(mid_price);

                let order_req = OrderRequest {
                    symbol: Symbol::from(self.symbol.convert_to_smolstr()),
                    side: Side::Buy,
                    order_type: OrderType::Limit,
                    price: buy_price,
                    amount: self.position_size,
                    timestamp: None,
                    exchange_params: Default::default(),
                };
                debug!(
                    "Returning BUY order request: symbol={symbol}, price={price}, amount={amount}",
                    symbol = order_req.symbol,
                    price = order_req.price,
                    amount = order_req.amount
                );
                return Ok(Some(order_req));
            } else {
                debug!("Max position reached ({current_qty} + {position_size} > {max_position}), skipping buy",
                    current_qty = current_qty,
                    position_size = self.position_size,
                    max_position = self.max_position
                );
            }
        } else if self.should_sell(mid_price, avg_price) && current_qty > Decimal::ZERO {
            // We have position and price is high - sell
            let sell_qty = self.position_size.min(current_qty);

            let diff = if avg_price > Decimal::ZERO {
                (mid_price - avg_price) / avg_price * Decimal::from(100)
            } else {
                Decimal::ZERO
            };
            debug!(
                "SELL signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%, qty={sell_qty} BTC"
            );

            // Use best bid as sell price
            let sell_price = orderbook.best_bid_price().unwrap_or(mid_price);

            let order_req = OrderRequest {
                symbol: Symbol::from(self.symbol.convert_to_smolstr()),
                side: Side::Sell,
                order_type: OrderType::Limit,
                price: sell_price,
                amount: sell_qty,
                timestamp: None,
                exchange_params: Default::default(),
            };
            debug!(
                "Returning SELL order request: symbol={symbol}, price={price}, amount={amount}",
                symbol = order_req.symbol,
                price = order_req.price,
                amount = order_req.amount
            );
            return Ok(Some(order_req));
        } else {
            // No trade signal - log why (only periodically to reduce noise)
            // Calculate how close we are to buy/sell signals
            // For buy: need price_diff_from_avg <= -buy_threshold (price below average by threshold)
            // For sell: need price_diff_from_avg >= sell_threshold (price above average by threshold)

            let buy_diff_needed = if price_diff_from_avg < Decimal::ZERO {
                // Price is below average, calculate how much more drop needed
                (-price_diff_from_avg) - self.buy_threshold
            } else {
                // Price is above average, would need to drop this much + threshold
                price_diff_from_avg + self.buy_threshold
            };

            let sell_diff_needed = if price_diff_from_avg > Decimal::ZERO {
                // Price is above average, calculate how much more rise needed
                price_diff_from_avg - self.sell_threshold
            } else {
                // Price is below average, would need to rise this much + threshold
                (-price_diff_from_avg) + self.sell_threshold
            };

            // Show when we're very close to a signal (within 0.02% of threshold to reduce noise)
            if buy_diff_needed.abs() < Decimal::new(2, 2)
                || sell_diff_needed.abs() < Decimal::new(2, 2)
            {
                // Close to a signal - log it with clearer messaging
                // When values are negative, it means we're very close (just need a tiny bit more)
                let buy_msg = if buy_diff_needed <= Decimal::ZERO {
                    format!("TRIGGERED! ({:.3}% below threshold)", buy_diff_needed.abs())
                } else {
                    format!("need {:.3}% more drop", buy_diff_needed)
                };

                let sell_msg = if sell_diff_needed <= Decimal::ZERO {
                    // Negative means we're below threshold, need more rise (show absolute value)
                    format!("need {:.3}% more rise", sell_diff_needed.abs())
                } else {
                    format!("need {:.3}% more rise", sell_diff_needed)
                };

                debug!(
                    "Close to signal: price_diff={diff:.3}%, buy: {buy_msg}, sell: {sell_msg}, position={position}",
                    diff = price_diff_from_avg,
                    buy_msg = buy_msg,
                    sell_msg = sell_msg,
                    position = current_qty
                );
            } else {
                // Only log when far from signal (every 60 updates) to reduce noise
                if self.price_history.len().is_multiple_of(60) {
                    debug!(
                        "No trade signal: price_diff={diff:.3}%, thresholds={buy_threshold:.2}%/{sell_threshold:.2}%, position={position}",
                        diff = price_diff_from_avg,
                        buy_threshold = self.buy_threshold,
                        sell_threshold = self.sell_threshold,
                        position = current_qty
                    );
                }
            }
        }

        Ok(None)
    }

    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError> {
        // Use the strategy's symbol (Pair) directly instead of parsing from Symbol
        // This avoids the symbol parsing bug and ensures consistency
        let pair = self.symbol.clone();

        // Now get state and update
        let state = self.get_state_mut();

        // Update order status with the actual status from the execution report
        state.update_order_status(
            &order.order_id,
            order.status.clone(),
            Some(order.filled_amount),
        );

        // Handle different order statuses appropriately
        match order.status {
            OrderStatus::Filled => {
                // Order is fully filled - calculate incremental quantity change
                // filled_amount is cumulative, so we need to get the previous filled amount
                // and calculate the delta to avoid double-counting if we already processed partial fills
                let previous_filled = state
                    .active_orders
                    .get(&order.order_id)
                    .map(|o| o.filled_amount)
                    .unwrap_or(rust_decimal::Decimal::ZERO);

                // Calculate incremental fill amount (delta)
                let incremental_fill = order.filled_amount - previous_filled;

                if incremental_fill > rust_decimal::Decimal::ZERO {
                    let qty_change = match order.side {
                        Side::Buy => incremental_fill,
                        Side::Sell => -incremental_fill,
                    };

                    // Remove from active orders (now that it's fully filled)
                    state.remove_order(&order.order_id);

                    // Update position with incremental change (final fill)
                    state.update_position(pair, qty_change, order.price);
                } else {
                    // Already fully processed, just remove the order
                    state.remove_order(&order.order_id);
                }
            }
            OrderStatus::PartiallyFilled => {
                // Order is partially filled - calculate incremental quantity change
                // filled_amount is cumulative, so we need to get the previous filled amount
                // and calculate the delta
                let previous_filled = state
                    .active_orders
                    .get(&order.order_id)
                    .map(|o| o.filled_amount)
                    .unwrap_or(rust_decimal::Decimal::ZERO);

                // Calculate incremental fill amount (delta)
                let incremental_fill = order.filled_amount - previous_filled;

                if incremental_fill > rust_decimal::Decimal::ZERO {
                    let qty_change = match order.side {
                        Side::Buy => incremental_fill,
                        Side::Sell => -incremental_fill,
                    };

                    // Keep order active (don't remove) and update position with incremental change
                    state.update_position(pair, qty_change, order.price);
                }
            }
            OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired => {
                // Order was cancelled/rejected/expired - remove from active orders
                state.remove_order(&order.order_id);
            }
            _ => {
                // Other statuses (New, etc.) - just update status, don't modify position
            }
        }

        // Log fills and trades
        match order.status {
            OrderStatus::Filled => {
                info!(
                    "Order FILLED: {side} {filled}/{total} {symbol} @ {price}",
                    side = order.side,
                    filled = order.filled_amount,
                    total = order.amount,
                    symbol = order.symbol,
                    price = order.price
                );
            }
            OrderStatus::PartiallyFilled => {
                info!(
                    "Order PARTIALLY FILLED: {side} {filled}/{total} {symbol} @ {price}",
                    side = order.side,
                    filled = order.filled_amount,
                    total = order.amount,
                    symbol = order.symbol,
                    price = order.price
                );
            }
            _ => {
                debug!(
                    "Order status updated: {side} {symbol} -> {status:?}",
                    side = order.side,
                    symbol = order.symbol,
                    status = order.status
                );
            }
        }

        Ok(())
    }

    fn get_state(&self) -> &TradingState {
        &self.state
    }

    fn get_state_mut(&mut self) -> &mut TradingState {
        &mut self.state
    }
}
