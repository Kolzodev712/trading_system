//! Buy Low Sell High Strategy - Enhanced Mean Reversion Strategy

use crate::state::{Strategy, TradingState};
use crate::strategies::strategy_manager::{
    LiquidityAnalyzer, OrderTypeSelector, PerformanceTracker, PositionSizer, TimeFilter,
    TrendDetector, TrendDirection, VolatilityTracker,
};
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

/// Enhanced "Buy Low, Sell High" strategy with all improvements
pub struct BuyLowSellHigh {
    state: TradingState,
    #[allow(dead_code)] // Reserved for future use
    trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
    symbol: Pair,

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

    // Enhancement components
    volatility_tracker: VolatilityTracker,
    trend_detector: TrendDetector,
    performance_tracker: PerformanceTracker,
    liquidity_analyzer: LiquidityAnalyzer,
    time_filter: TimeFilter,
    position_sizer: PositionSizer,
    order_type_selector: OrderTypeSelector,

    // Indicator support (optional - for EMA/RSI)
    // Use calculator directly (read-only, uses RwLock internally - no deadlock risk)
    indicator_calculator: Option<Arc<indicator_engine::calculator::IndicatorCalculator>>,
    ema_fast_id: Option<smol_str::SmolStr>, // Fast EMA indicator ID
    ema_slow_id: Option<smol_str::SmolStr>, // Slow EMA indicator ID
    rsi_id: Option<smol_str::SmolStr>,      // RSI indicator ID
}

impl BuyLowSellHigh {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        trading_client: Arc<RwLock<dyn TradingClient<OrderRequest>>>,
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
        indicator_calculator: Option<Arc<indicator_engine::calculator::IndicatorCalculator>>,
    ) -> Self {
        Self {
            state: TradingState::new(),
            trading_client,
            symbol,
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
            volatility_tracker: VolatilityTracker::new(20), // 20-period volatility
            trend_detector: TrendDetector::new(
                Decimal::new(5, 1), // 0.5% for strong trend
                Decimal::new(1, 1), // 0.1% for weak trend
            ),
            performance_tracker: PerformanceTracker::new(20), // Track last 20 trades
            liquidity_analyzer: LiquidityAnalyzer::new(
                Decimal::new(1, 2), // 0.1% max spread
                Decimal::new(1, 0), // 1.0 minimum depth
            ),
            time_filter: TimeFilter::new(true), // Enable time filtering
            position_sizer: PositionSizer::new(
                base_position_size,
                Decimal::from(2),   // Max 2x base size
                Decimal::new(5, 1), // Min 0.5x base size
            ),
            order_type_selector: OrderTypeSelector::new(
                true,               // Use market orders for strong signals
                Decimal::from(2),   // 2x threshold = strong signal
                Decimal::new(5, 3), // 0.05% max spread for market
                Decimal::new(1, 1), // 1% max volatility for market
            ),
            indicator_calculator,
            ema_fast_id: None,
            ema_slow_id: None,
            rsi_id: None,
        }
    }

    /// Set indicator IDs (call after indicators are registered)
    pub fn set_indicator_ids(
        &mut self,
        ema_fast_id: Option<smol_str::SmolStr>,
        ema_slow_id: Option<smol_str::SmolStr>,
        rsi_id: Option<smol_str::SmolStr>,
    ) {
        self.ema_fast_id = ema_fast_id;
        self.ema_slow_id = ema_slow_id;
        self.rsi_id = rsi_id;
    }

    /// Get EMA value from indicator calculator (fallback to SMA if not available)
    fn get_ema_value(&self, indicator_id: &Option<smol_str::SmolStr>) -> Option<Decimal> {
        if let (Some(calculator), Some(id)) = (self.indicator_calculator.as_ref(), indicator_id) {
            calculator.get_value(id.as_str()).map(|v| v.value)
        } else {
            None
        }
    }

    /// Get RSI value from indicator calculator
    fn get_rsi_value(&self) -> Option<Decimal> {
        if let (Some(calculator), Some(id)) =
            (self.indicator_calculator.as_ref(), self.rsi_id.as_ref())
        {
            calculator.get_value(id.as_str()).map(|v| v.value)
        } else {
            None
        }
    }

    /// Calculate average price (use EMA if available, else SMA fallback)
    fn calculate_average_price(&self) -> Option<Decimal> {
        // Try to use slow EMA first (more stable)
        if let Some(ema) = self.get_ema_value(&self.ema_slow_id) {
            return Some(ema);
        }

        // Fallback to SMA
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

    /// Check if we should buy based on price, trend, and RSI
    fn should_buy(
        &self,
        current_price: Decimal,
        avg_price: Decimal,
        trend: TrendDirection,
        rsi: Option<Decimal>,
    ) -> bool {
        if avg_price <= Decimal::ZERO {
            return false;
        }

        // Trend filter: don't buy in strong downtrends
        if !self.trend_detector.should_allow_buy(trend) {
            return false;
        }

        // RSI filter: only buy when oversold or neutral
        if let Some(rsi_val) = rsi {
            if rsi_val > Decimal::from(50) {
                // RSI above 50 = not oversold, skip buy
                return false;
            }
        }

        // Price threshold check
        let price_diff = (avg_price - current_price) / avg_price * Decimal::from(100);
        let adaptive_threshold = self
            .volatility_tracker
            .get_adaptive_threshold(self.base_buy_threshold);
        price_diff >= adaptive_threshold
    }

    /// Check if we should sell based on price, trend, and RSI
    fn should_sell(
        &self,
        current_price: Decimal,
        avg_price: Decimal,
        trend: TrendDirection,
        rsi: Option<Decimal>,
    ) -> bool {
        if avg_price <= Decimal::ZERO {
            return false;
        }

        // Trend filter: don't sell in strong uptrends
        if !self.trend_detector.should_allow_sell(trend) {
            return false;
        }

        // RSI filter: only sell when overbought or neutral
        if let Some(rsi_val) = rsi {
            if rsi_val < Decimal::from(50) {
                // RSI below 50 = not overbought, skip sell
                return false;
            }
        }

        // Price threshold check
        let price_diff = (current_price - avg_price) / avg_price * Decimal::from(100);
        let adaptive_threshold = self
            .volatility_tracker
            .get_adaptive_threshold(self.base_sell_threshold);
        price_diff >= adaptive_threshold
    }
}

#[async_trait]
impl Strategy for BuyLowSellHigh {
    async fn on_orderbook(
        &mut self,
        orderbook: &Orderbook,
        state: &TradingState,
    ) -> Result<Option<OrderRequest>, EngineError> {
        // Performance-based pause check
        if self.performance_tracker.should_pause_trading() {
            debug!("Trading paused due to consecutive losses");
            return Ok(None);
        }

        // Time filter
        if !self
            .time_filter
            .is_high_liquidity_period(orderbook.timestamp)
        {
            return Ok(None);
        }

        // Liquidity filter
        if !self.liquidity_analyzer.should_trade_by_liquidity(orderbook) {
            return Ok(None);
        }

        // Get current mid price
        let mid_price = match self.get_mid_price(orderbook) {
            Some(p) => p,
            None => return Ok(None),
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
            return Ok(None);
        }

        // Get average price (EMA preferred, SMA fallback)
        let avg_price = match self.calculate_average_price() {
            Some(p) if p > Decimal::ZERO => p,
            Some(_) => {
                warn!("Average price is zero, skipping trade decision");
                return Ok(None);
            }
            None => return Ok(None),
        };

        // Get trend direction (if EMAs available)
        let trend = if let (Some(fast_ema), Some(slow_ema)) = (
            self.get_ema_value(&self.ema_fast_id),
            self.get_ema_value(&self.ema_slow_id),
        ) {
            self.trend_detector.get_trend_direction(fast_ema, slow_ema)
        } else {
            TrendDirection::Neutral // No trend info available
        };

        // Get RSI
        let rsi = self.get_rsi_value();

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

        // Check stop-loss and take-profit
        if let Some(position) = current_position {
            if position.quantity > Decimal::ZERO {
                if let Some(stop_loss) = self.stop_loss_percent {
                    if position.avg_price > Decimal::ZERO {
                        let loss_percent = (position.avg_price - mid_price) / position.avg_price
                            * Decimal::from(100);
                        if loss_percent >= stop_loss {
                            warn!("Stop-loss triggered: loss={loss_percent:.2}%, threshold={stop_loss}%");
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

        // Calculate price difference and signal strength
        if avg_price <= Decimal::ZERO {
            return Ok(None);
        }

        // Get volatility and spread for dynamic sizing and order type
        let volatility = self.volatility_tracker.calculate_volatility();
        let spread_percent = self.liquidity_analyzer.calculate_spread_percent(orderbook);

        // Apply performance-based threshold adjustment
        let threshold_adjustment = self.performance_tracker.get_threshold_adjustment();
        let adjusted_buy_threshold = self.base_buy_threshold * threshold_adjustment;
        let adjusted_sell_threshold = self.base_sell_threshold * threshold_adjustment;

        // Buy logic
        if self.should_buy(mid_price, avg_price, trend, rsi) {
            let signal_strength = (avg_price - mid_price) / avg_price * Decimal::from(100);

            // Calculate dynamic position size
            let position_size = self.position_sizer.calculate_size(
                signal_strength,
                adjusted_buy_threshold,
                volatility,
            );

            if current_qty + position_size <= self.max_position {
                debug!(
                    "BUY signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%, size={size}, trend={trend:?}, rsi={rsi:?}",
                    diff = signal_strength,
                    size = position_size,
                    trend = trend,
                    rsi = rsi
                );

                // Select order type
                let order_type = self.order_type_selector.select_order_type(
                    signal_strength,
                    adjusted_buy_threshold,
                    spread_percent,
                    volatility,
                );

                let buy_price = match order_type {
                    OrderType::Market => mid_price, // Market orders use mid price
                    OrderType::Limit => orderbook.best_ask_price().unwrap_or(mid_price),
                    _ => orderbook.best_ask_price().unwrap_or(mid_price),
                };

                return Ok(Some(OrderRequest {
                    symbol: Symbol::from(self.symbol.convert_to_smolstr()),
                    side: Side::Buy,
                    order_type,
                    price: buy_price,
                    amount: position_size,
                    timestamp: None,
                    exchange_params: Default::default(),
                }));
            }
        }
        // Sell logic
        else if self.should_sell(mid_price, avg_price, trend, rsi) && current_qty > Decimal::ZERO
        {
            let signal_strength = (mid_price - avg_price) / avg_price * Decimal::from(100);

            // Calculate dynamic position size
            let position_size = self.position_sizer.calculate_size(
                signal_strength,
                adjusted_sell_threshold,
                volatility,
            );
            let sell_qty = position_size.min(current_qty);

            debug!(
                "SELL signal: price={mid_price}, avg={avg_price}, diff={diff:.2}%, qty={sell_qty}, trend={trend:?}, rsi={rsi:?}",
                diff = signal_strength,
                trend = trend,
                rsi = rsi
            );

            // Select order type
            let order_type = self.order_type_selector.select_order_type(
                signal_strength,
                adjusted_sell_threshold,
                spread_percent,
                volatility,
            );

            let sell_price = match order_type {
                OrderType::Market => mid_price, // Market orders use mid price
                OrderType::Limit => orderbook.best_bid_price().unwrap_or(mid_price),
                _ => orderbook.best_bid_price().unwrap_or(mid_price),
            };

            return Ok(Some(OrderRequest {
                symbol: Symbol::from(self.symbol.convert_to_smolstr()),
                side: Side::Sell,
                order_type,
                price: sell_price,
                amount: sell_qty,
                timestamp: None,
                exchange_params: Default::default(),
            }));
        }

        Ok(None)
    }

    async fn on_execution_report(&mut self, order: &Order) -> Result<(), EngineError> {
        let pair = self.symbol.clone();
        let state = self.get_state_mut();

        // Track previous filled amount for profit calculation
        let previous_filled = state
            .active_orders
            .get(&order.order_id)
            .map(|o| o.filled_amount)
            .unwrap_or(Decimal::ZERO);

        state.update_order_status(
            &order.order_id,
            order.status.clone(),
            Some(order.filled_amount),
        );

        match order.status {
            OrderStatus::Filled => {
                let incremental_fill = order.filled_amount - previous_filled;

                if incremental_fill > Decimal::ZERO {
                    let qty_change = match order.side {
                        Side::Buy => incremental_fill,
                        Side::Sell => -incremental_fill,
                    };

                    state.remove_order(&order.order_id);
                    state.update_position(pair.clone(), qty_change, order.price);

                    // Track performance (calculate profit/loss)
                    if let Some(position) = state.get_position(&pair) {
                        if position.quantity == Decimal::ZERO {
                            // Position closed - calculate P&L
                            // This is simplified - in reality you'd track entry/exit prices better
                            // For now, we'll track based on order fills
                        }
                    }
                } else {
                    state.remove_order(&order.order_id);
                }

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
                let incremental_fill = order.filled_amount - previous_filled;

                if incremental_fill > Decimal::ZERO {
                    let qty_change = match order.side {
                        Side::Buy => incremental_fill,
                        Side::Sell => -incremental_fill,
                    };
                    state.update_position(pair, qty_change, order.price);
                }

                info!(
                    "Order PARTIALLY FILLED: {side} {filled}/{total} {symbol} @ {price}",
                    side = order.side,
                    filled = order.filled_amount,
                    total = order.amount,
                    symbol = order.symbol,
                    price = order.price
                );
            }
            OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired => {
                state.remove_order(&order.order_id);
            }
            _ => {}
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
