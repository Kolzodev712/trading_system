use super::strategy::{Strategy, TradingState};
use core_logic::{
    data::{
        base_data::{OrderType, Pair, Side},
        private_data::{Order, OrderRequest},
        public_data::Orderbook,
    },
    error::EngineError,
    traits::TradingClient,
};
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    symbol: Pair,

    // Price history for moving average
    price_history: Vec<Decimal>,
    max_history: usize,

    // Strategy parameters
    buy_threshold: Decimal,        // Buy when price is X% below average
    sell_threshold: Decimal,      // Sell when price is X% above average
    position_size: Decimal,        // Fixed position size per trade
    max_position: Decimal,         // Maximum total position size
    max_active_orders: usize,      // Maximum concurrent orders
    stop_loss_percent: Option<Decimal>,  // Stop loss percentage (e.g., 5.0 for 5%)
    take_profit_percent: Option<Decimal>, // Take profit percentage (e.g., 10.0 for 10%)
}

impl BuyLowSellHigh {
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
        if orderbook.bids.is_empty() || orderbook.asks.is_empty() {
            return None;
        }
        let best_bid = orderbook.bids[0];
        let best_ask = orderbook.asks[0];
        Some((best_bid + best_ask) / Decimal::from(2))
    }

    fn should_buy(&self, current_price: Decimal, avg_price: Decimal) -> bool {
        let price_diff = (avg_price - current_price) / avg_price * Decimal::from(100);
        price_diff >= self.buy_threshold
    }

    fn should_sell(&self, current_price: Decimal, avg_price: Decimal) -> bool {
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
        let min_history = 20; // Default minimum
        if self.price_history.len() < min_history {
            debug!(
                "Building price history: {current}/{target}",
                current = self.price_history.len(),
                target = min_history
            );
            return Ok(None);
        }

        let avg_price = match self.calculate_average_price() {
            Some(p) => p,
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
                    let loss_percent = (position.avg_price - mid_price) / position.avg_price * Decimal::from(100);
                    if loss_percent >= stop_loss {
                        warn!("Stop-loss triggered: loss={loss_percent:.2}%, threshold={stop_loss}%");
                        let sell_price = if !orderbook.bids.is_empty() {
                            orderbook.bids[0]
                        } else {
                            mid_price
                        };
                        return Ok(Some(OrderRequest {
                            symbol: self.symbol.clone().into(),
                            side: Side::Sell,
                            order_type: OrderType::Limit,
                            price: sell_price,
                            amount: position.quantity,
                            timestamp: None,
                            exchange_params: Default::default(),
                        }));
                    }
                }

                // Check take-profit
                if let Some(take_profit) = self.take_profit_percent {
                    let profit_percent = (mid_price - position.avg_price) / position.avg_price * Decimal::from(100);
                    if profit_percent >= take_profit {
                        info!("Take-profit triggered: profit={profit_percent:.2}%, threshold={take_profit}%");
                        let sell_price = if !orderbook.bids.is_empty() {
                            orderbook.bids[0]
                        } else {
                            mid_price
                        };
                        return Ok(Some(OrderRequest {
                            symbol: self.symbol.clone().into(),
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

        // Decision logic
        if self.should_buy(mid_price, avg_price) {
            // Check if we can buy more
            if current_qty + self.position_size <= self.max_position {
                let diff = (avg_price - mid_price) / avg_price * Decimal::from(100);
                info!("BUY signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%");

                // Use best ask as buy price (market order would be better, but limit is safer)
                let buy_price = if !orderbook.asks.is_empty() {
                    orderbook.asks[0]
                } else {
                    mid_price
                };

                return Ok(Some(OrderRequest {
                    symbol: self.symbol.clone().into(),
                    side: Side::Buy,
                    order_type: OrderType::Limit,
                    price: buy_price,
                    amount: self.position_size,
                    timestamp: None,
                    exchange_params: Default::default(),
                }));
            } else {
                debug!("Max position reached, skipping buy");
            }
        } else if self.should_sell(mid_price, avg_price) && current_qty > Decimal::ZERO {
            // We have position and price is high - sell
            let sell_qty = self.position_size.min(current_qty);

            let diff = (mid_price - avg_price) / avg_price * Decimal::from(100);
            info!(
                "SELL signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%, qty={sell_qty}"
            );

            // Use best bid as sell price
            let sell_price = if !orderbook.bids.is_empty() {
                orderbook.bids[0]
            } else {
                mid_price
            };

            return Ok(Some(OrderRequest {
                symbol: self.symbol.clone().into(),
                side: Side::Sell,
                order_type: OrderType::Limit,
                price: sell_price,
                amount: sell_qty,
                timestamp: None,
                exchange_params: Default::default(),
            }));
        }

        Ok(None)
    }

    async fn on_execution_report(
        &mut self,
        order: &Order,
        state: &mut TradingState,
    ) -> Result<(), EngineError> {
        // Remove from active orders
        state.remove_order(&order.order_id);

        // Update position
        let qty_change = match order.side {
            Side::Buy => order.amount,
            Side::Sell => -order.amount,
        };

        state.update_position(order.symbol.clone(), qty_change, order.price);

        info!(
            "Order executed: {side} {amount} {symbol} @ {price}",
            side = order.side,
            amount = order.amount,
            symbol = order.symbol,
            price = order.price
        );

        Ok(())
    }

    fn get_state(&self) -> &TradingState {
        &self.state
    }

    fn get_state_mut(&mut self) -> &mut TradingState {
        &mut self.state
    }
}
