//! Strategy manager and common strategy utilities
//!
//! This module provides:
//! - StrategyManager: manages multiple strategies
//! - Common utilities: volatility tracking, trend detection, performance tracking, etc.

use crate::state::Strategy;
use chrono::{DateTime, Datelike, Timelike, Utc};
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    types::{common::Pair, exchange::Order, market_data::Orderbook},
};

/// Strategy identifier: strategy_name + symbol
pub type StrategyId = String;

/// Manages multiple strategies with per-strategy enable/disable support
pub struct StrategyManager {
    strategies: Vec<Arc<RwLock<dyn Strategy>>>,
    /// Maps strategy identifier (strategy_name:symbol) to enabled state
    strategy_enabled: Arc<RwLock<HashMap<StrategyId, bool>>>,
    /// Maps strategy index to strategy identifier
    strategy_ids: Vec<StrategyId>,
    /// Maps strategy index to trading pair (for efficient symbol matching)
    strategy_pairs: Vec<Pair>,
    /// Index: Maps trading pair to strategy indices (for O(1) lookup)
    pair_to_indices: HashMap<Pair, Vec<usize>>,
    /// Global trading control - if disabled, all strategies are disabled regardless of individual state
    trading_control: Option<Arc<tokio::sync::RwLock<bool>>>,
}

impl StrategyManager {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
            strategy_enabled: Arc::new(RwLock::new(HashMap::new())),
            strategy_ids: Vec::new(),
            strategy_pairs: Vec::new(),
            pair_to_indices: HashMap::new(),
            trading_control: None,
        }
    }

    /// Set global trading control (hierarchical control)
    pub fn with_trading_control(mut self, trading_control: Arc<tokio::sync::RwLock<bool>>) -> Self {
        self.trading_control = Some(trading_control);
        self
    }

    /// Add a strategy with its identifier and trading pair
    pub async fn add_strategy(
        &mut self,
        strategy: Arc<RwLock<dyn Strategy>>,
        strategy_id: StrategyId,
        pair: Pair,
    ) {
        // Default to disabled
        let mut enabled = self.strategy_enabled.write().await;
        enabled.insert(strategy_id.clone(), false);
        drop(enabled);

        let idx = self.strategies.len();
        self.strategy_ids.push(strategy_id);
        self.strategy_pairs.push(pair.clone());
        self.strategies.push(strategy);

        // Index by pair for efficient lookup
        self.pair_to_indices
            .entry(pair)
            .or_insert_with(Vec::new)
            .push(idx);
    }

    /// Set enabled state for a strategy
    pub async fn set_strategy_enabled(&self, strategy_id: &StrategyId, enabled: bool) {
        let mut enabled_map = self.strategy_enabled.write().await;
        enabled_map.insert(strategy_id.clone(), enabled);
    }

    /// Get enabled state for a strategy
    pub async fn is_strategy_enabled(&self, strategy_id: &StrategyId) -> bool {
        let enabled_map = self.strategy_enabled.read().await;
        enabled_map.get(strategy_id).copied().unwrap_or(false) // Default to disabled
    }

    /// Get a reference to the enabled map (for direct access to avoid nested locks)
    /// This allows callers to clone the Arc and access the map without holding the manager lock
    pub fn get_enabled_map(&self) -> &Arc<RwLock<HashMap<StrategyId, bool>>> {
        &self.strategy_enabled
    }

    /// Generate strategy ID from name and symbol
    pub fn make_strategy_id(strategy_name: &str, symbol: &Pair) -> StrategyId {
        format!("{}:{}", strategy_name, symbol)
    }

    pub async fn on_orderbook(
        &self,
        orderbook: &Orderbook,
    ) -> Result<Vec<trading_core::types::exchange::OrderRequest>, EngineError> {
        // Log entry point (periodically)
        static ENTRY_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let entry_count = ENTRY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let should_log_entry = entry_count % 20 == 0;

        if should_log_entry {
            info!(
                "[StrategyManager] on_orderbook called for {}",
                orderbook.pair
            );
        }

        // Check global trading control first - if disabled, all strategies are disabled
        if let Some(ref trading_control) = self.trading_control {
            let enabled = *trading_control.read().await;
            if should_log_entry {
                info!(
                    "[StrategyManager] Global trading control check: enabled={}",
                    enabled
                );
            }
            if !enabled {
                if should_log_entry {
                    info!("[StrategyManager] Global trading disabled, returning empty");
                }
                return Ok(Vec::new()); // Global trading disabled, no orders
            }
        } else if should_log_entry {
            info!("[StrategyManager] No trading control set, proceeding");
        }

        let mut requests = Vec::new();

        // Get only strategies that match this orderbook's symbol (O(1) lookup)
        let matching_indices = self
            .pair_to_indices
            .get(&orderbook.pair)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        if matching_indices.is_empty() {
            // No strategies for this symbol - this is fine, just return empty
            return Ok(requests);
        }

        // Get enabled states for matching strategies only
        let enabled_map = self.strategy_enabled.read().await;
        let enabled_states: Vec<(usize, bool)> = matching_indices
            .iter()
            .map(|&idx| {
                let strategy_id = &self.strategy_ids[idx];
                let enabled = enabled_map.get(strategy_id).copied().unwrap_or(false);
                (idx, enabled)
            })
            .collect();
        drop(enabled_map);

        // Log periodically which strategies are being checked
        static CALL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let call_count = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let should_log = call_count % 20 == 0;

        if should_log {
            info!(
                "[StrategyManager] Processing orderbook for {}, found {} matching strategy(ies)",
                orderbook.pair,
                matching_indices.len()
            );
        }

        // Only process strategies that match this symbol
        for (idx, enabled) in enabled_states {
            let strategy_id = self
                .strategy_ids
                .get(idx)
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            if !enabled {
                if should_log {
                    info!(
                        "[StrategyManager] Strategy {} is disabled, skipping",
                        strategy_id
                    );
                }
                continue; // Skip disabled strategies
            }

            if should_log {
                info!(
                    "[StrategyManager] Calling strategy.on_orderbook() for {}",
                    strategy_id
                );
            }

            let strategy = &self.strategies[idx];
            let mut strat = strategy.write().await;
            let state = strat.get_state().clone();
            match strat.on_orderbook(orderbook, &state).await {
                Ok(Some(req)) => {
                    info!(
                        "[StrategyManager] ✅ Strategy {} returned order request: {} {} @ {} x {}",
                        strategy_id, req.side, req.symbol, req.price, req.amount
                    );
                    requests.push(req);
                }
                Ok(None) => {
                    if should_log {
                        info!(
                            "[StrategyManager] Strategy {} returned None (no order)",
                            strategy_id
                        );
                    }
                }
                Err(e) => {
                    error!("[StrategyManager] Strategy {} error: {}", strategy_id, e);
                    return Err(e);
                }
            }
        }

        if should_log {
            info!(
                "[StrategyManager] Total requests from matching strategies: {}",
                requests.len()
            );
        }

        Ok(requests)
    }

    pub async fn on_execution_report(&self, order: &Order) -> Result<(), EngineError> {
        for strategy in &self.strategies {
            let mut strat_guard = strategy.write().await;
            strat_guard.on_execution_report(order).await?;
        }
        Ok(())
    }

    pub fn get_strategies(&self) -> &[Arc<RwLock<dyn Strategy>>] {
        &self.strategies
    }
}

impl Default for StrategyManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Common Strategy Utilities
// ============================================================================

/// Volatility tracker for adaptive thresholds
pub struct VolatilityTracker {
    returns: VecDeque<Decimal>,
    period: usize,
}

impl VolatilityTracker {
    pub fn new(period: usize) -> Self {
        Self {
            returns: VecDeque::with_capacity(period),
            period,
        }
    }

    /// Update with new price and calculate return
    pub fn update(&mut self, current_price: Decimal, previous_price: Option<Decimal>) {
        if let Some(prev) = previous_price {
            if prev > Decimal::ZERO {
                let return_pct = (current_price - prev) / prev * Decimal::from(100);
                self.returns.push_back(return_pct);
                if self.returns.len() > self.period {
                    self.returns.pop_front();
                }
            }
        }
    }

    /// Calculate current volatility (variance of returns as proxy for volatility)
    pub fn calculate_volatility(&self) -> Decimal {
        if self.returns.len() < 2 {
            return Decimal::ZERO;
        }

        // Calculate mean return
        let mean: Decimal =
            self.returns.iter().sum::<Decimal>() / Decimal::from(self.returns.len());

        // Calculate variance (proxy for volatility)
        let variance: Decimal = self
            .returns
            .iter()
            .map(|r| {
                let diff = *r - mean;
                diff * diff
            })
            .sum::<Decimal>()
            / Decimal::from(self.returns.len());

        variance
    }

    /// Get adaptive threshold based on volatility
    pub fn get_adaptive_threshold(&self, base_threshold: Decimal) -> Decimal {
        let volatility = self.calculate_volatility();
        // Normalize volatility (assume 1% = normal, scale from there)
        let vol_multiplier = volatility / Decimal::from(100);
        // Clamp multiplier between 0.5x and 2x
        let clamped_multiplier = vol_multiplier.max(Decimal::new(5, 1)).min(Decimal::from(2));
        base_threshold * clamped_multiplier
    }
}

/// Trend direction based on EMA comparison
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrendDirection {
    StrongUp,   // Don't sell
    WeakUp,     // Can sell
    Neutral,    // Trade both ways
    WeakDown,   // Can buy
    StrongDown, // Don't buy
}

/// Trend detector using fast/slow EMA comparison
pub struct TrendDetector {
    strong_threshold: Decimal, // e.g., 0.5% for strong trend
    weak_threshold: Decimal,   // e.g., 0.1% for weak trend
}

impl TrendDetector {
    pub fn new(strong_threshold: Decimal, weak_threshold: Decimal) -> Self {
        Self {
            strong_threshold,
            weak_threshold,
        }
    }

    /// Determine trend direction from fast and slow EMA
    pub fn get_trend_direction(&self, fast_ema: Decimal, slow_ema: Decimal) -> TrendDirection {
        if slow_ema <= Decimal::ZERO {
            return TrendDirection::Neutral;
        }

        let diff_pct = (fast_ema - slow_ema) / slow_ema * Decimal::from(100);

        if diff_pct > self.strong_threshold {
            TrendDirection::StrongUp
        } else if diff_pct > self.weak_threshold {
            TrendDirection::WeakUp
        } else if diff_pct < -self.strong_threshold {
            TrendDirection::StrongDown
        } else if diff_pct < -self.weak_threshold {
            TrendDirection::WeakDown
        } else {
            TrendDirection::Neutral
        }
    }

    /// Check if we should allow buy based on trend
    pub fn should_allow_buy(&self, trend: TrendDirection) -> bool {
        matches!(
            trend,
            TrendDirection::Neutral | TrendDirection::WeakDown | TrendDirection::WeakUp
        )
    }

    /// Check if we should allow sell based on trend
    pub fn should_allow_sell(&self, trend: TrendDirection) -> bool {
        matches!(
            trend,
            TrendDirection::Neutral | TrendDirection::WeakUp | TrendDirection::WeakDown
        )
    }
}

/// Performance tracker for adaptive thresholds
pub struct PerformanceTracker {
    recent_trades: VecDeque<TradeResult>,
    max_trades: usize,
    consecutive_losses: usize,
}

#[derive(Debug, Clone)]
pub struct TradeResult {
    pub profit: Decimal,
    pub timestamp: u64,
}

impl PerformanceTracker {
    pub fn new(max_trades: usize) -> Self {
        Self {
            recent_trades: VecDeque::with_capacity(max_trades),
            max_trades,
            consecutive_losses: 0,
        }
    }

    pub fn record_trade(&mut self, profit: Decimal, timestamp: u64) {
        let is_loss = profit < Decimal::ZERO;

        if is_loss {
            self.consecutive_losses += 1;
        } else {
            self.consecutive_losses = 0;
        }

        self.recent_trades
            .push_back(TradeResult { profit, timestamp });
        if self.recent_trades.len() > self.max_trades {
            self.recent_trades.pop_front();
        }
    }

    pub fn calculate_win_rate(&self) -> Option<Decimal> {
        if self.recent_trades.is_empty() {
            return None;
        }

        let wins = self
            .recent_trades
            .iter()
            .filter(|t| t.profit > Decimal::ZERO)
            .count();

        Some(Decimal::from(wins) / Decimal::from(self.recent_trades.len()) * Decimal::from(100))
    }

    /// Get threshold adjustment based on performance
    /// Returns multiplier (e.g., 1.2 = widen by 20%, 0.8 = tighten by 20%)
    pub fn get_threshold_adjustment(&self) -> Decimal {
        // Pause after too many consecutive losses
        if self.consecutive_losses >= 5 {
            return Decimal::from(2); // Double thresholds (effectively pause)
        }

        if let Some(win_rate) = self.calculate_win_rate() {
            if win_rate < Decimal::from(40) {
                // Low win rate - widen thresholds (fewer trades, higher quality)
                Decimal::new(12, 1) // 1.2x
            } else if win_rate > Decimal::from(60) {
                // High win rate - tighten thresholds (more trades)
                Decimal::new(8, 1) // 0.8x
            } else {
                Decimal::ONE // No adjustment
            }
        } else {
            Decimal::ONE
        }
    }

    pub fn should_pause_trading(&self) -> bool {
        self.consecutive_losses >= 5
    }
}

/// Spread and liquidity analyzer
pub struct LiquidityAnalyzer {
    max_spread_percent: Decimal,  // e.g., 0.1% max spread
    min_orderbook_depth: Decimal, // Minimum depth at best bid/ask
}

impl LiquidityAnalyzer {
    pub fn new(max_spread_percent: Decimal, min_orderbook_depth: Decimal) -> Self {
        Self {
            max_spread_percent,
            min_orderbook_depth,
        }
    }

    /// Calculate bid-ask spread percentage
    pub fn calculate_spread_percent(&self, orderbook: &Orderbook) -> Option<Decimal> {
        let bid = orderbook.best_bid_price()?;
        let ask = orderbook.best_ask_price()?;
        let mid = (bid + ask) / Decimal::from(2);
        if mid > Decimal::ZERO {
            Some((ask - bid) / mid * Decimal::from(100))
        } else {
            None
        }
    }

    /// Calculate orderbook depth (total volume at best bid/ask)
    pub fn calculate_orderbook_depth(&self, orderbook: &Orderbook) -> Decimal {
        // Get volume at best bid (first bid level)
        let bid_volume = orderbook
            .bids
            .first()
            .map(|(_, qty)| *qty)
            .unwrap_or(Decimal::ZERO);

        // Get volume at best ask (first ask level)
        let ask_volume = orderbook
            .asks
            .first()
            .map(|(_, qty)| *qty)
            .unwrap_or(Decimal::ZERO);

        bid_volume + ask_volume
    }

    /// Check if liquidity conditions are good for trading
    pub fn should_trade_by_liquidity(&self, orderbook: &Orderbook) -> bool {
        let spread = match self.calculate_spread_percent(orderbook) {
            Some(s) => s,
            None => return false,
        };

        let depth = self.calculate_orderbook_depth(orderbook);

        spread < self.max_spread_percent && depth >= self.min_orderbook_depth
    }
}

/// Time-based liquidity filter
pub struct TimeFilter {
    enabled: bool,
}

impl TimeFilter {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Check if current time is high liquidity period
    pub fn is_high_liquidity_period(&self, timestamp: u64) -> bool {
        if !self.enabled {
            return true; // No filtering if disabled
        }

        let dt = DateTime::<Utc>::from_timestamp(timestamp as i64, 0);
        if let Some(dt) = dt {
            let weekday = dt.weekday();
            let hour = dt.hour();

            // Skip weekends (Saturday = 6, Sunday = 7 in number_from_monday)
            if weekday.number_from_monday() == 6 || weekday.number_from_monday() == 7 {
                return false;
            }

            // Skip low-volume hours (2-6 AM UTC for crypto, adjust as needed)
            // Most crypto volume is 8 AM - 10 PM UTC
            if (2..6).contains(&hour) {
                return false;
            }

            true
        } else {
            true // Default to allowing if timestamp invalid
        }
    }
}

/// Dynamic position size calculator
pub struct PositionSizer {
    base_size: Decimal,
    max_size_multiplier: Decimal, // e.g., 2.0 = can go up to 2x base size
    min_size_multiplier: Decimal, // e.g., 0.5 = can go down to 0.5x base size
}

impl PositionSizer {
    pub fn new(
        base_size: Decimal,
        max_size_multiplier: Decimal,
        min_size_multiplier: Decimal,
    ) -> Self {
        Self {
            base_size,
            max_size_multiplier,
            min_size_multiplier,
        }
    }

    /// Calculate position size based on signal strength and volatility
    pub fn calculate_size(
        &self,
        signal_strength: Decimal, // How far from average (percentage)
        threshold: Decimal,       // Base threshold
        volatility: Decimal,      // Current volatility
    ) -> Decimal {
        // Confidence based on signal strength (cap at 2x threshold)
        let confidence = (signal_strength / threshold)
            .min(Decimal::from(2))
            .max(Decimal::new(5, 1)); // At least 0.5x

        // Volatility adjustment (smaller in high vol)
        let vol_adjustment = Decimal::ONE / (Decimal::ONE + volatility / Decimal::from(100));
        let clamped_vol = vol_adjustment
            .max(self.min_size_multiplier)
            .min(self.max_size_multiplier);

        let size = self.base_size * confidence * clamped_vol;
        size.max(self.base_size * self.min_size_multiplier)
            .min(self.base_size * self.max_size_multiplier)
    }
}

/// Order type selector (Market vs Limit)
pub struct OrderTypeSelector {
    use_market_for_strong_signals: bool,
    strong_signal_multiplier: Decimal, // e.g., 2.0 = signal must be 2x threshold
    max_spread_for_market: Decimal,    // e.g., 0.05% max spread for market orders
    max_volatility_for_market: Decimal, // e.g., 1% max volatility for market orders
}

impl OrderTypeSelector {
    pub fn new(
        use_market_for_strong_signals: bool,
        strong_signal_multiplier: Decimal,
        max_spread_for_market: Decimal,
        max_volatility_for_market: Decimal,
    ) -> Self {
        Self {
            use_market_for_strong_signals,
            strong_signal_multiplier,
            max_spread_for_market,
            max_volatility_for_market,
        }
    }

    /// Select order type based on signal strength, spread, and volatility
    pub fn select_order_type(
        &self,
        signal_strength: Decimal,
        threshold: Decimal,
        spread_percent: Option<Decimal>,
        volatility: Decimal,
    ) -> trading_core::types::common::OrderType {
        if !self.use_market_for_strong_signals {
            return trading_core::types::common::OrderType::Limit;
        }

        let is_strong_signal = signal_strength >= threshold * self.strong_signal_multiplier;
        let spread_ok = spread_percent
            .map(|s| s < self.max_spread_for_market)
            .unwrap_or(false);
        let vol_ok = volatility < self.max_volatility_for_market;

        if is_strong_signal && spread_ok && vol_ok {
            trading_core::types::common::OrderType::Market
        } else {
            trading_core::types::common::OrderType::Limit
        }
    }
}
