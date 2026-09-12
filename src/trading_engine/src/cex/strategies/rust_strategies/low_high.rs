//! Buy Low Sell High Strategy - Enhanced Mean Reversion Strategy

use crate::cex::intent::{StrategyContext, StrategyIntent};
use crate::cex::state::Strategy;
use crate::cex::strategies::strategy_manager::{
    LiquidityAnalyzer, PositionSizer, StrategyManager, TimeFilter, VolatilityTracker,
};
use crate::order_validation::get_trading_rules;
use asset_engine::AssetService;
use async_trait::async_trait;
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::{debug, info, warn};
use trading_core::{
    error::EngineError,
    types::{
        common::{Exchange, Instrument, InstrumentType, OrderStatus, Pair},
        exchange::Order,
        market_data::Orderbook,
    },
};

/// Enhanced "Buy Low, Sell High" strategy with all improvements
pub struct BuyLowSellHigh {
    symbol: Pair,

    // Asset service for exchange-agnostic trading rules
    asset_service: Option<Arc<AssetService>>,
    exchange: Exchange,
    // Config fallback values (used when asset service doesn't have data)
    config_min_order_size: Decimal,
    config_step_size: Decimal,
    config_tick_size: Decimal,
    config_min_notional: Decimal,

    // Price history (for volatility tracking and fallback)
    price_history: Vec<Decimal>,
    max_history: usize,
    min_price_history: usize,

    // Base strategy parameters
    base_buy_threshold: Decimal, // Base buy threshold (will be adjusted by volatility)
    base_sell_threshold: Decimal, // Base sell threshold (will be adjusted by volatility)
    max_position: Decimal,
    max_active_orders: usize,
    stop_loss_percent: Option<Decimal>,
    take_profit_percent: Option<Decimal>,

    volatility_tracker: VolatilityTracker,
    liquidity_analyzer: LiquidityAnalyzer,
    time_filter: TimeFilter,
    position_sizer: PositionSizer,
}

impl BuyLowSellHigh {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        symbol: Pair,
        base_position_size: Decimal,
        max_position: Decimal,
        base_buy_threshold: Decimal,
        base_sell_threshold: Decimal,
        stop_loss_percent: Option<Decimal>,
        take_profit_percent: Option<Decimal>,
        max_active_orders: usize,
        price_history_size: usize,
        min_price_history: usize,
        asset_service: Option<Arc<AssetService>>,
        exchange: Exchange,
        config_min_order_size: Decimal,
        config_step_size: Decimal,
        config_tick_size: Decimal,
        config_min_notional: Decimal,
    ) -> Self {
        Self {
            symbol,
            asset_service,
            exchange,
            config_min_order_size,
            config_step_size,
            config_tick_size,
            config_min_notional,
            price_history: Vec::new(),
            max_history: price_history_size,
            min_price_history,
            base_buy_threshold,
            base_sell_threshold,
            max_position,
            max_active_orders,
            stop_loss_percent,
            take_profit_percent,
            // Initialize enhancement components with defaults
            volatility_tracker: VolatilityTracker::new(20),
            liquidity_analyzer: LiquidityAnalyzer::new(Decimal::new(1, 2), Decimal::new(1, 0)),
            time_filter: TimeFilter::new(false),
            position_sizer: PositionSizer::new(
                base_position_size,
                Decimal::from(2),
                Decimal::new(5, 1),
            ),
        }
    }

    fn strategy_id(&self) -> String {
        StrategyManager::make_strategy_id("BuyLowSellHigh", &self.symbol)
    }

    fn hold(&self) -> StrategyIntent {
        StrategyIntent::hold(self.strategy_id())
    }

    fn target(&self, qty: Decimal, instrument_type: InstrumentType) -> StrategyIntent {
        StrategyIntent::target_position(
            self.strategy_id(),
            Instrument::new(instrument_type, self.symbol.clone()),
            qty,
        )
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

    fn should_buy(&self, current_price: Decimal, avg_price: Decimal) -> bool {
        if avg_price <= Decimal::ZERO {
            return false;
        }
        let price_diff = (avg_price - current_price) / avg_price * Decimal::from(100);
        price_diff >= self.base_buy_threshold
    }

    fn should_sell(&self, current_price: Decimal, avg_price: Decimal) -> bool {
        if avg_price <= Decimal::ZERO {
            return false;
        }
        let price_diff = (current_price - avg_price) / avg_price * Decimal::from(100);
        price_diff >= self.base_sell_threshold
    }
}

#[async_trait]
impl Strategy for BuyLowSellHigh {
    async fn evaluate(&mut self, ctx: &StrategyContext<'_>) -> Result<StrategyIntent, EngineError> {
        let orderbook = ctx.orderbook;
        let state = ctx.account;
        // Log that we're processing orderbooks (every 20th)
        static CALL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let call_count = CALL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let should_log = call_count.is_multiple_of(20);
        if should_log {
            info!(
                "[Strategy {}] Processing orderbook #{}",
                self.symbol, call_count
            );
        }

        // Time filter
        if !self
            .time_filter
            .is_high_liquidity_period(orderbook.timestamp)
        {
            if should_log {
                debug!("[Strategy {}] Blocked by time filter", self.symbol);
            }
            return Ok(self.hold());
        }

        // Liquidity filter
        if !self.liquidity_analyzer.should_trade_by_liquidity(orderbook) {
            if should_log {
                let spread = self.liquidity_analyzer.calculate_spread_percent(orderbook);
                debug!(
                    "[Strategy {}] Blocked by liquidity filter (spread={:?})",
                    self.symbol, spread
                );
            }
            return Ok(self.hold());
        }

        // Get current mid price
        let mid_price = match self.get_mid_price(orderbook) {
            Some(p) => p,
            None => return Ok(self.hold()),
        };

        // Update price history and volatility tracker
        let previous_price = self.price_history.last().copied();
        self.price_history.push(mid_price);
        if self.price_history.len() > self.max_history {
            self.price_history.remove(0);
        }
        self.volatility_tracker.update(mid_price, previous_price);

        // Need minimum price history
        if self.price_history.len() < self.min_price_history {
            return Ok(self.hold());
        }

        // Get average price (EMA preferred, SMA fallback)
        let avg_price = match self.calculate_average_price() {
            Some(p) if p > Decimal::ZERO => p,
            Some(_) => {
                warn!("Average price is zero, skipping trade decision");
                return Ok(self.hold());
            }
            None => return Ok(self.hold()),
        };

        // Log price comparison periodically (every 20 orderbooks to see what's happening)
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let should_log = count.is_multiple_of(20);
        if should_log {
            let price_diff_percent = (avg_price - mid_price) / avg_price * Decimal::from(100);
            info!(
                "[Strategy {}] Price check: current={}, avg={}, diff={:.3}%, threshold={:.3}%, history={}/{}",
                self.symbol,
                mid_price,
                avg_price,
                price_diff_percent,
                self.base_buy_threshold,
                self.price_history.len(),
                self.min_price_history
            );
        }

        // Check active orders limit
        let active_count = state.get_active_order_count(&self.symbol);
        if active_count >= self.max_active_orders {
            if should_log {
                debug!(
                    "[Strategy {}] Blocked by active orders limit: {}/{}",
                    self.symbol, active_count, self.max_active_orders
                );
            }
            return Ok(self.hold());
        }

        // Inventory = filled position. Effective includes working orders so we do not rebuy.
        let current_position = state.get_position(&self.symbol);
        let inventory = state.position_qty(&self.symbol);
        let current_qty = state.effective_position_qty(&self.symbol);

        // Check stop-loss and take-profit
        // Only check if we have a position for THIS symbol (safety check)
        if let Some(position) = current_position {
            // CRITICAL: Verify the position is for THIS symbol (prevent cross-symbol bugs)
            // Also verify the position's symbol matches what we requested (double-check)
            if position.symbol != self.symbol {
                warn!(
                    "[Strategy {}] Position symbol mismatch: expected {}, got {}. Skipping stop-loss check.",
                    self.symbol, self.symbol, position.symbol
                );
                // Continue to normal trading logic - skip stop-loss entirely
            } else if position.quantity > Decimal::ZERO {
                // Additional safety: verify position symbol matches again before using it
                if position.symbol.base != self.symbol.base
                    || position.symbol.quote != self.symbol.quote
                {
                    warn!(
                        "[Strategy {}] Position base/quote mismatch: expected {}/{}, got {}/{}. Skipping stop-loss.",
                        self.symbol, self.symbol.base, self.symbol.quote, position.symbol.base, position.symbol.quote
                    );
                    // Continue to normal trading logic - skip stop-loss entirely
                } else {
                    // Position symbol matches - safe to use for stop-loss and take-profit
                    // Additional sanity check: if entry price is way off from current price (>50% difference),
                    // something is wrong - skip stop-loss to prevent false triggers
                    let price_diff_ratio = if position.avg_price > Decimal::ZERO {
                        (mid_price - position.avg_price).abs() / position.avg_price
                    } else {
                        Decimal::ZERO
                    };

                    if price_diff_ratio > Decimal::new(5, 1) {
                        // 0.5 = 50%
                        warn!(
                            "[Strategy {}] Position entry price ({}) is >50% different from current price ({}). Skipping stop-loss to prevent false triggers.",
                            self.symbol, position.avg_price, mid_price
                        );
                        // Continue to normal trading logic - skip stop-loss entirely
                    } else if let Some(stop_loss) = self.stop_loss_percent {
                        if position.avg_price > Decimal::ZERO {
                            // Calculate loss: (entry_price - current_price) / entry_price * 100
                            // For a buy position: if current_price < entry_price, we have a loss
                            let loss_percent = if mid_price < position.avg_price {
                                (position.avg_price - mid_price) / position.avg_price
                                    * Decimal::from(100)
                            } else {
                                Decimal::ZERO // We have a gain, not a loss
                            };

                            if loss_percent >= stop_loss {
                                warn!(
                                    "[Strategy {}] Stop-loss triggered: loss={loss_percent:.2}%, threshold={stop_loss}%, entry={}, current={}",
                                    self.symbol, position.avg_price, mid_price
                                );
                                return Ok(self.target(Decimal::ZERO, ctx.instrument_type.clone()));
                            }
                        }
                    }

                    // Check take-profit (only if we have a valid position for THIS symbol)
                    if let Some(take_profit) = self.take_profit_percent {
                        if position.avg_price > Decimal::ZERO {
                            let profit_percent = (mid_price - position.avg_price)
                                / position.avg_price
                                * Decimal::from(100);
                            if profit_percent >= take_profit {
                                info!("Take-profit triggered: profit={profit_percent:.2}%, threshold={take_profit}%");
                                return Ok(self.target(Decimal::ZERO, ctx.instrument_type.clone()));
                            }
                        }
                    }
                } // Close else block for position symbol match
            } // Close else if position.quantity > Decimal::ZERO
        } // Close if let Some(position)

        // Calculate price difference and signal strength
        if avg_price <= Decimal::ZERO {
            return Ok(self.hold());
        }

        let volatility = self.volatility_tracker.calculate_volatility();
        let buy_signal = self.should_buy(mid_price, avg_price);

        if !buy_signal && should_log {
            let price_diff = (avg_price - mid_price) / avg_price * Decimal::from(100);
            info!(
                "[Strategy {}] Buy blocked: price_diff={:.3}% (need {:.3}%)",
                self.symbol, price_diff, self.base_buy_threshold
            );
        }

        if buy_signal {
            let signal_strength = (avg_price - mid_price) / avg_price * Decimal::from(100);

            // Calculate dynamic position size
            let position_size = self.position_sizer.calculate_size(
                signal_strength,
                self.base_buy_threshold,
                volatility,
            );

            if should_log {
                info!(
                    "[Strategy {}] BUY SIGNAL: price_diff={:.3}%, signal_strength={:.3}%, position_size={}, current_qty={}, max_position={}, active_orders={}/{}",
                    self.symbol,
                    (avg_price - mid_price) / avg_price * Decimal::from(100),
                    signal_strength,
                    position_size,
                    current_qty,
                    self.max_position,
                    active_count,
                    self.max_active_orders
                );
            }

            if current_qty + position_size <= self.max_position {
                info!(
                    "[Strategy {}] BUY: price={}, avg={}, diff={:.3}%, size={}",
                    self.symbol, mid_price, avg_price, signal_strength, position_size
                );

                let buy_price = orderbook.best_ask_price().unwrap_or(mid_price);

                // Get trading rules from Asset Engine (exchange-agnostic)
                let rules = get_trading_rules(
                    self.asset_service.as_ref(),
                    self.exchange.clone(),
                    &self.symbol,
                    self.config_min_order_size,
                    self.config_step_size,
                    self.config_tick_size,
                    self.config_min_notional,
                )
                .await;

                // Ensure order meets minimum notional
                // Calculate minimum quantity needed: min_notional / price
                // Then round UP to step_size to ensure it still meets minimum after executor's floor rounding
                let min_quantity_raw = rules.min_notional / buy_price;
                let min_quantity = if rules.step_size > Decimal::ZERO {
                    // Round UP: ceil(min_quantity / step_size) * step_size
                    ((min_quantity_raw / rules.step_size).ceil() * rules.step_size)
                        .max(rules.min_order_size)
                } else {
                    min_quantity_raw.max(rules.min_order_size)
                };

                // Use the larger of calculated position_size or minimum required quantity
                let final_amount = position_size.max(min_quantity);

                // But don't exceed max_position
                let final_amount = if current_qty + final_amount <= self.max_position {
                    final_amount
                } else {
                    // If it would exceed max_position, check if we can at least place min_quantity
                    let remaining_capacity = self.max_position - current_qty;
                    if remaining_capacity >= min_quantity {
                        remaining_capacity.min(final_amount)
                    } else {
                        // Can't place order that meets minimum notional
                        if should_log {
                            warn!(
                                "[Strategy {}] Cannot place order: would exceed max_position (current={}, needed={}, max={})",
                                self.symbol,
                                current_qty,
                                final_amount,
                                self.max_position
                            );
                        }
                        return Ok(self.hold());
                    }
                };

                // Round final_amount UP to step_size to ensure it still meets minimum notional after executor's floor rounding
                let final_amount_rounded = if rules.step_size > Decimal::ZERO {
                    ((final_amount / rules.step_size).ceil() * rules.step_size).max(min_quantity)
                } else {
                    final_amount.max(min_quantity)
                };

                let target_qty = (current_qty + final_amount_rounded).min(self.max_position);
                info!(
                    "[Strategy {}] BUY intent: current={} delta={} target={}",
                    self.symbol, current_qty, final_amount_rounded, target_qty
                );
                return Ok(self.target(target_qty, ctx.instrument_type.clone()));
            } else if should_log {
                warn!(
                    "[Strategy {}] Buy signal blocked: position would exceed max (current={}, size={}, max={})",
                    self.symbol,
                    current_qty,
                    position_size,
                    self.max_position
                );
            }
        }
        // Sell logic
        else if self.should_sell(mid_price, avg_price) && inventory > Decimal::ZERO {
            let signal_strength = (mid_price - avg_price) / avg_price * Decimal::from(100);
            let position_size = self.position_sizer.calculate_size(
                signal_strength,
                self.base_sell_threshold,
                volatility,
            );
            let sell_qty = position_size.min(inventory);
            let sell_price = orderbook.best_bid_price().unwrap_or(mid_price);

            // Get trading rules from Asset Engine (exchange-agnostic)
            let rules = get_trading_rules(
                self.asset_service.as_ref(),
                self.exchange.clone(),
                &self.symbol,
                self.config_min_order_size,
                self.config_step_size,
                self.config_tick_size,
                self.config_min_notional,
            )
            .await;

            // Ensure order meets minimum notional
            // Calculate minimum quantity needed: min_notional / price
            // Then round UP to step_size to ensure it still meets minimum after executor's floor rounding
            let min_quantity_raw = rules.min_notional / sell_price;
            let min_quantity = if rules.step_size > Decimal::ZERO {
                // Round UP: ceil(min_quantity / step_size) * step_size
                ((min_quantity_raw / rules.step_size).ceil() * rules.step_size)
                    .max(rules.min_order_size)
            } else {
                min_quantity_raw.max(rules.min_order_size)
            };
            let final_sell_qty = sell_qty.max(min_quantity).min(inventory);

            // If we don't have enough to meet minimum notional, skip the order
            if final_sell_qty < min_quantity {
                debug!(
                    "[Strategy {}] Sell order skipped: insufficient quantity to meet minimum notional (have={}, need={})",
                    self.symbol,
                    inventory,
                    min_quantity
                );
                return Ok(self.hold());
            }

            // Round final_sell_qty UP to step_size to ensure it still meets minimum notional after executor's floor rounding
            let final_sell_qty_rounded = if rules.step_size > Decimal::ZERO {
                ((final_sell_qty / rules.step_size).ceil() * rules.step_size)
                    .max(min_quantity)
                    .min(inventory)
            } else {
                final_sell_qty.max(min_quantity)
            };

            debug!(
                "SELL signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%, qty={final_sell_qty_rounded}",
                diff = signal_strength,
            );

            let target_qty = (inventory - final_sell_qty_rounded).max(Decimal::ZERO);
            debug!(
                "[Strategy {}] SELL intent: current={} delta={} target={}",
                self.symbol, inventory, final_sell_qty_rounded, target_qty
            );
            return Ok(self.target(target_qty, ctx.instrument_type.clone()));
        }

        Ok(self.hold())
    }

    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError> {
        match order.status {
            OrderStatus::Filled => {
                info!(
                    "Order FILLED: {side} {filled}/{total} {symbol} @ {price}",
                    side = order.side,
                    filled = order.filled_amount,
                    total = order.amount,
                    symbol = order.instrument.symbol(),
                    price = order.price
                );
            }
            OrderStatus::PartiallyFilled => {
                info!(
                    "Order PARTIALLY FILLED: {side} {filled}/{total} {symbol} @ {price}",
                    side = order.side,
                    filled = order.filled_amount,
                    total = order.amount,
                    symbol = order.instrument.symbol(),
                    price = order.price
                );
            }
            _ => {}
        }
        Ok(())
    }
}
