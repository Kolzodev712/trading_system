//! Monitoring data collection and streaming
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::Status;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;

use super::converters;

/// Service data needed for monitoring operations
pub struct MonitoringService {
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub trading_state_manager: Arc<TradingStateManager>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
    pub indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    /// Strategy info (only set in strategies mode)
    pub strategy_info: Option<StrategyInfo>,
}

/// Strategy information for monitoring
#[derive(Clone)]
pub struct StrategyInfo {
    pub strategy_name: String,
    pub symbol: trading_core::types::common::Pair,
    pub max_active_orders: usize,
}

impl MonitoringService {
    /// Collect monitoring data from all sources
    pub async fn collect_monitoring_data(&self) -> Result<trading_cli::MonitoringData, Status> {
        let start = std::time::Instant::now();
        // Collect all data in parallel to avoid sequential blocking
        let ((orders, balances), positions, indicators, trades) = tokio::join!(
            // Get orders and balances from ExchangeStateManager
            async {
                let exchange_state_arc = self.exchange_state_manager.get_state();
                let exchange_state = exchange_state_arc.read().await;

                // Build order_id -> level_id mapping from LevelManager
                let level_manager = self.level_manager.read().await;
                let mut order_to_level: HashMap<String, (String, String)> = HashMap::new();

                for level in level_manager.get_all_levels() {
                    let level_id = level.config.id.to_string();
                    let order_type_str = converters::order_type_to_string(&level.config.order_type);

                    for order_id in &level.active_orders {
                        order_to_level.insert(
                            order_id.as_str().to_string(),
                            (level_id.clone(), order_type_str.to_string()),
                        );
                    }
                }
                drop(level_manager); // Release lock before continuing

                // Get open orders (active orders from exchange)
                let orders: Vec<trading_cli::Order> = exchange_state
                    .get_open_orders()
                    .iter()
                    .map(|order| {
                        let order_id_str = order.order_id.as_str().to_string();
                        let (level_id, order_type) = order_to_level
                            .get(&order_id_str)
                            .cloned()
                            .unwrap_or_else(|| (String::new(), "Limit".to_string())); // Default to Limit if not found

                        trading_cli::Order {
                            order_id: order_id_str,
                            symbol: order.symbol.to_string(),
                            side: format!("{:?}", order.side),
                            order_type,
                            price: order.price.to_string(),
                            quantity: order.amount.to_string(),
                            status: format!("{:?}", order.status),
                            level_id,
                            timestamp: order.internal_created_at as i64,
                        }
                    })
                    .collect();

                // Get balances (from WebSocket updates)
                let balances: Vec<trading_cli::Balance> = exchange_state
                    .get_balances()
                    .iter()
                    .map(|b| trading_cli::Balance {
                        asset: b.symbol.to_string(),
                        free: b.free.to_string(),
                        used: b.used.to_string(),
                        locked: b.used.to_string(), // used is the locked amount
                        total: b.total.to_string(),
                    })
                    .collect();

                (orders, balances)
            },
            // Get positions from TradingStateManager
            async {
                let trading_state_arc = self.trading_state_manager.get_state();
                let trading_state = trading_state_arc.read().await;
                let positions: Vec<trading_cli::Position> = trading_state
                    .positions
                    .values()
                    .map(|pos| trading_cli::Position {
                        symbol: pos.symbol.convert_to_smolstr().to_string(),
                        quantity: pos.quantity.to_string(),
                        avg_price: pos.avg_price.to_string(),
                    })
                    .collect();

                if !positions.is_empty() {
                    tracing::debug!(
                        "[gRPC] get_positions: {} position(s) found",
                        positions.len()
                    );
                }

                positions
            },
            // Get indicators
            async { self.get_indicators().await.unwrap_or_default() },
            // Get trades
            async { self.get_trades().await.unwrap_or_default() }
        );

        // Aggregated data
        let aggregated = trading_cli::AggregatedData {
            overall: Some(trading_cli::OverallStatistics {
                total_orders: orders.len() as i32,
                total_positions: positions.len() as i32,
                total_position_value: String::new(), // TODO: Calculate
                total_pnl: String::new(),            // TODO: Calculate
                total_balance: balances
                    .iter()
                    .find(|b| b.asset == "USDT" || b.asset == "USD")
                    .map(|b| b.total.clone())
                    .unwrap_or_default(),
                active_pairs: 1,  // TODO: Get from config
                active_levels: 0, // TODO: Get from LevelManager
            }),
            per_pair: std::collections::HashMap::new(),
            per_level: std::collections::HashMap::new(),
        };

        let elapsed = start.elapsed();
        if elapsed.as_millis() > 100 {
            tracing::warn!(
                "[gRPC] collect_monitoring_data took {}ms (slow!)",
                elapsed.as_millis()
            );
        }

        // Collect strategy status if in strategies mode
        let strategy_status = if let Some(ref strategy_info) = self.strategy_info {
            self.collect_strategy_status(strategy_info, &positions)
                .await
        } else {
            None
        };

        Ok(trading_cli::MonitoringData {
            orders,
            balances,
            trades,
            positions,
            indicators,
            aggregated: Some(aggregated),
            strategy: strategy_status,
        })
    }

    /// Get recent trades from ExchangeStateManager (already populated via WebSocket)
    pub async fn get_trades(&self) -> Result<Vec<trading_cli::Trade>, Status> {
        // Get trades directly from ExchangeStateManager - they're already there from WebSocket updates
        let state_arc = self.exchange_state_manager.get_state();
        let trades: Vec<trading_core::types::exchange::Trade> = {
            let exchange_state = state_arc.read().await;
            // Clone the trades to own them before dropping the lock
            exchange_state
                .get_trades()
                .iter()
                .map(|t| (*t).clone())
                .collect()
        };

        // Convert to protobuf format, sorted by timestamp (most recent first)
        let mut converted: Vec<trading_cli::Trade> = trades
            .iter()
            .map(|t| trading_cli::Trade {
                trade_id: t.trade_id.to_string(),
                order_id: t.order_id.as_str().to_string(),
                symbol: t.pair.convert_to_smolstr().to_string(),
                side: format!("{:?}", t.side),
                price: t.price.to_string(),
                quantity: t.quantity.to_string(),
                timestamp: t.timestamp as i64,
            })
            .collect();

        // Sort by timestamp descending (most recent first)
        converted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Limit to most recent 100 trades for display
        converted.truncate(100);

        Ok(converted)
    }

    /// Get indicators from IndicatorService
    pub async fn get_indicators(&self) -> Result<Vec<trading_cli::IndicatorValue>, Status> {
        // Access calculator directly - it's thread-safe, no locking needed
        let values = self.indicator_calculator.get_all_current_values();

        let mut converted: Vec<trading_cli::IndicatorValue> = values
            .into_iter()
            .map(|v| trading_cli::IndicatorValue {
                indicator_id: v.indicator_id.to_string(),
                indicator_name: v.indicator_name.to_string(),
                pair: v.pair.to_string(),
                value: v.value.to_string(),
                timestamp: v.timestamp as i64,
                period: v.period as i32,
                is_composite: v.is_composite,
            })
            .collect();

        // Sort by timestamp descending and limit
        converted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        converted.truncate(100);

        Ok(converted)
    }

    /// Collect strategy status information
    async fn collect_strategy_status(
        &self,
        strategy_info: &StrategyInfo,
        positions: &[trading_cli::Position],
    ) -> Option<trading_cli::StrategyStatus> {
        // Find position for this strategy's symbol
        let symbol_str = strategy_info.symbol.convert_to_smolstr().to_string();
        let position = positions.iter().find(|p| p.symbol == symbol_str);

        // Get active orders count for this symbol
        let trading_state_arc = self.trading_state_manager.get_state();
        let trading_state = trading_state_arc.read().await;
        let active_orders = trading_state
            .active_orders
            .values()
            .filter(|o| {
                let order_symbol: smol_str::SmolStr = o.symbol.clone().into();
                order_symbol.as_str() == symbol_str
            })
            .count();

        // Calculate P&L if we have a position
        // TODO: Get current price from MarketDataStateManager for accurate P&L
        let (position_quantity, position_avg_price, unrealized_pnl, unrealized_pnl_percent) =
            if let Some(pos) = position {
                (
                    pos.quantity.clone(),
                    pos.avg_price.clone(),
                    "N/A".to_string(), // Will calculate when we have market data access
                    "N/A".to_string(),
                )
            } else {
                (
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0%".to_string(),
                )
            };

        Some(trading_cli::StrategyStatus {
            strategy_name: strategy_info.strategy_name.clone(),
            symbol: symbol_str,
            position_quantity,
            position_avg_price,
            unrealized_pnl,
            unrealized_pnl_percent,
            active_orders: active_orders as i32,
            max_active_orders: strategy_info.max_active_orders as i32,
            last_signal: "None".to_string(), // TODO: Track last signal from strategy
            signal_strength: String::new(),  // TODO: Get from strategy if available
        })
    }
}
