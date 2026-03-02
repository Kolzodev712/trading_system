//! Monitoring data collection and streaming
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::Status;
use tracing::debug;
use trading_core::utils::hours_ago;
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;
use trading_engine::strategies::strategy_manager::StrategyManager;

use super::converters;

/// Service data needed for monitoring operations
pub struct MonitoringService {
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub trading_state_manager: Arc<TradingStateManager>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
    pub market_data_state_manager: Arc<market_data_engine::state::manager::MarketDataStateManager>,
    pub indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    /// Strategy info (only set in strategies mode, supports multiple)
    pub strategy_infos: Vec<StrategyInfo>,
    /// Strategy manager (only set in strategies mode, for checking enabled status)
    pub strategy_manager: Option<Arc<RwLock<StrategyManager>>>,
    /// Instrument type (Spot or Futures) - determines which TUI view to show
    pub instrument_type: trading_core::types::common::InstrumentType,
    /// Exchange name for CLI display (e.g. "Binance", "Bybit")
    pub exchange: String,
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
        debug!(
            "[gRPC] collect_monitoring_data: Starting collection ({} strategy infos)",
            self.strategy_infos.len()
        );
        // Collect all data in parallel to avoid sequential blocking
        let ((orders, balances), positions, indicators, trades, orderbooks) = tokio::join!(
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

                let orders: Vec<trading_cli::Order> = exchange_state
                    .get_open_orders()
                    .iter()
                    .filter(|order| {
                        // Use exchange_created_at if available, otherwise fallback to internal_created_at
                        let order_timestamp = order
                            .exchange_created_at
                            .unwrap_or(order.internal_created_at);
                        order_timestamp >= hours_ago(24)
                    })
                    .map(|order| {
                        let order_id_str = order.order_id.as_str().to_string();
                        let (level_id, order_type) = order_to_level
                            .get(&order_id_str)
                            .cloned()
                            .unwrap_or_else(|| (String::new(), "Limit".to_string())); // Default to Limit if not found

                        trading_cli::Order {
                            order_id: order_id_str,
                            symbol: order.instrument.symbol().to_string(),
                            side: format!("{:?}", order.side),
                            order_type,
                            price: order.price.to_string(),
                            quantity: order.amount.to_string(),
                            status: format!("{:?}", order.status),
                            level_id,
                            timestamp: order
                                .exchange_created_at
                                .unwrap_or(order.internal_created_at)
                                as i64,
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
            // Get positions from ExchangeStateManager (for futures) or TradingStateManager (for spot)
            async {
                let exchange_state_arc = self.exchange_state_manager.get_state();
                let exchange_state = exchange_state_arc.read().await;
                let exchange_positions: Vec<trading_cli::Position> = exchange_state
                    .get_positions()
                    .iter()
                    .map(|pos| {
                        let side = if pos.quantity.is_sign_positive() {
                            "Long"
                        } else {
                            "Short"
                        };
                        trading_cli::Position {
                            symbol: pos.instrument.symbol().to_string(),
                            quantity: pos.quantity.to_string(),
                            avg_price: pos.avg_price.to_string(),
                            side: side.to_string(),
                            mark_price: pos.mark_price.map(|p| p.to_string()).unwrap_or_default(),
                            unrealized_pnl: pos.unrealized_pnl.to_string(),
                            leverage: pos.leverage.map(|l| format!("{}x", l)).unwrap_or_default(),
                            margin_used: pos
                                .initial_margin
                                .map(|m| m.to_string())
                                .unwrap_or_default(),
                            maintenance_margin: pos
                                .maintenance_margin
                                .map(|m| m.to_string())
                                .unwrap_or_default(),
                            liquidation_price: pos
                                .liquidation_price
                                .map(|p| p.to_string())
                                .unwrap_or_default(),
                            realized_pnl: pos
                                .realized_pnl
                                .map(|p| p.to_string())
                                .unwrap_or_default(),
                        }
                    })
                    .collect();

                // Also get positions from TradingStateManager (for spot/strategy positions)
                let trading_state_arc = self.trading_state_manager.get_state();
                let trading_state = trading_state_arc.read().await;
                let trading_positions: Vec<trading_cli::Position> = trading_state
                    .positions
                    .values()
                    .map(|pos| trading_cli::Position {
                        symbol: pos.symbol.convert_to_smolstr().to_string(),
                        quantity: pos.quantity.to_string(),
                        avg_price: pos.avg_price.to_string(),
                        side: String::new(), // Trading engine positions don't have side
                        mark_price: String::new(),
                        unrealized_pnl: String::new(),
                        leverage: String::new(),
                        margin_used: String::new(),
                        maintenance_margin: String::new(),
                        liquidation_price: String::new(),
                        realized_pnl: String::new(),
                    })
                    .collect();

                // Combine both (exchange positions take precedence for same symbol)
                let mut all_positions = exchange_positions.clone();
                for trading_pos in trading_positions.clone() {
                    if !all_positions.iter().any(|p| p.symbol == trading_pos.symbol) {
                        all_positions.push(trading_pos);
                    }
                }

                if !all_positions.is_empty() {
                    tracing::debug!(
                        "[gRPC] get_positions: {} position(s) found ({} from exchange, {} from trading)",
                        all_positions.len(),
                        exchange_positions.len(),
                        trading_positions.len()
                    );
                }

                all_positions
            },
            // Get indicators
            async { self.get_indicators().await.unwrap_or_default() },
            // Get trades
            async { self.get_trades().await.unwrap_or_default() },
            // Get orderbooks
            async { self.get_orderbooks().await.unwrap_or_default() }
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

        // Collect strategy statuses for all strategies
        let mut strategy_statuses = Vec::new();
        for strategy_info in &self.strategy_infos {
            match self
                .collect_strategy_status(strategy_info, &positions)
                .await
            {
                Some(status) => {
                    strategy_statuses.push(status);
                }
                None => {
                    tracing::warn!(
                        "Failed to collect status for strategy {}:{}",
                        strategy_info.strategy_name,
                        strategy_info.symbol
                    );
                }
            }
        }
        tracing::debug!(
            "[gRPC] collect_monitoring_data: Collected {} strategy statuses from {} strategy infos, {} orders, {} balances, {} positions, {} indicators",
            strategy_statuses.len(),
            self.strategy_infos.len(),
            orders.len(),
            balances.len(),
            positions.len(),
            indicators.len()
        );

        // Convert instrument_type to string
        let instrument_type_str = match self.instrument_type {
            trading_core::types::common::InstrumentType::Spot => "Spot",
            trading_core::types::common::InstrumentType::Futures => "Futures",
            trading_core::types::common::InstrumentType::Derivitives => "Futures", // Treat derivatives as futures
        };

        let data = trading_cli::MonitoringData {
            orders,
            balances,
            trades,
            positions,
            indicators,
            aggregated: Some(aggregated),
            strategies: strategy_statuses,
            orderbooks,
            instrument_type: instrument_type_str.to_string(),
            exchange: self.exchange.clone(),
        };

        tracing::debug!("[gRPC] collect_monitoring_data: Returning data successfully");
        Ok(data)
    }

    /// Get recent trades from ExchangeStateManager (already populated via WebSocket)
    /// Only returns trades from the past 24 hours
    pub async fn get_trades(&self) -> Result<Vec<trading_cli::Trade>, Status> {
        // Only show trades from the past 24 hours
        let twenty_four_hours_ago = hours_ago(24);

        // Get trades directly from ExchangeStateManager - they're already there from WebSocket updates
        let state_arc = self.exchange_state_manager.get_state();
        let trades: Vec<trading_core::types::exchange::Trade> = {
            let exchange_state = state_arc.read().await;
            // Clone and filter trades from the past 24 hours
            exchange_state
                .get_trades()
                .iter()
                .filter(|t| t.timestamp >= twenty_four_hours_ago)
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

        // Limit to most recent 100 trades for display (after time filtering)
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
        // Convert Pair to string format: "BASE/QUOTE" (e.g., "ETH/USDC")
        let symbol_str = format!(
            "{}/{}",
            strategy_info.symbol.base, strategy_info.symbol.quote
        );
        tracing::debug!(
            "Collecting strategy status for {}:{} (symbol_str: {})",
            strategy_info.strategy_name,
            strategy_info.symbol,
            symbol_str
        );
        // Position symbols might be in "ETHUSDC" format, so check both
        let symbol_no_slash = format!(
            "{}{}",
            strategy_info.symbol.base, strategy_info.symbol.quote
        );
        let position = positions.iter().find(|p| {
            // Try both formats: "ETHUSDC" and "ETH/USDC"
            p.symbol == symbol_str || p.symbol == symbol_no_slash
        });

        // Get active orders count for this symbol
        let trading_state_arc = self.trading_state_manager.get_state();
        let trading_state = trading_state_arc.read().await;
        let active_orders = trading_state
            .active_orders
            .values()
            .filter(|o| {
                let order_symbol: smol_str::SmolStr = o.instrument.symbol().clone();
                // Order symbols might be in "ETHUSDC" format, so check both
                order_symbol.as_str() == symbol_str || order_symbol.as_str() == symbol_no_slash
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

        // Get strategy ID and enabled status
        let strategy_id =
            StrategyManager::make_strategy_id(&strategy_info.strategy_name, &strategy_info.symbol);

        // Get enabled status from strategy manager
        // We use is_strategy_enabled which handles the locking internally
        let enabled = if let Some(ref strategy_manager) = self.strategy_manager {
            strategy_manager
                .read()
                .await
                .is_strategy_enabled(&strategy_id)
                .await
        } else {
            true // Default to enabled if strategy manager not available
        };

        Some(trading_cli::StrategyStatus {
            strategy_id: strategy_id.clone(),
            strategy_name: strategy_info.strategy_name.clone(),
            symbol: symbol_str,
            enabled,
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

    /// Get orderbooks from MarketDataStateManager
    pub async fn get_orderbooks(&self) -> Result<Vec<trading_cli::Orderbook>, Status> {
        let state_arc = self.market_data_state_manager.get_state();
        let state = state_arc.read().await;

        // Get all configured pairs from symbol registry (not just pairs with orderbooks)
        // This ensures we show all pairs even if they haven't received updates yet
        let symbol_registry = self.market_data_state_manager.symbol_registry();
        let configured_pairs: Vec<trading_core::types::common::Pair> =
            symbol_registry.all_pairs().into_iter().cloned().collect();

        let pairs_with_data = state.get_pairs_with_orderbooks();

        debug!(
            "[gRPC] get_orderbooks: Registry has {} pairs: {:?}, State has {} pairs with data: {:?}",
            configured_pairs.len(),
            configured_pairs.iter().map(|p| format!("{}/{}", p.base, p.quote)).collect::<Vec<_>>(),
            pairs_with_data.len(),
            pairs_with_data.iter().map(|p| format!("{}/{}", p.base, p.quote)).collect::<Vec<_>>()
        );

        // Use configured pairs instead of just pairs with orderbooks
        let pairs = if configured_pairs.is_empty() {
            // Fallback to pairs with orderbooks if registry is empty
            pairs_with_data
        } else {
            configured_pairs
        };

        let mut orderbooks = Vec::new();

        for pair in pairs {
            // Get orderbook if available, otherwise create empty one
            let orderbook_opt = state.get_orderbook(&pair);

            let (bids, asks, best_bid, best_ask, mid_price, spread, timestamp) =
                if let Some(orderbook) = orderbook_opt {
                    // Get top 10 bids and asks
                    let bids: Vec<trading_cli::OrderbookLevel> = orderbook
                        .bids
                        .iter()
                        .take(10)
                        .map(|(price, qty)| trading_cli::OrderbookLevel {
                            price: price.to_string(),
                            quantity: qty.to_string(),
                        })
                        .collect();

                    let asks: Vec<trading_cli::OrderbookLevel> = orderbook
                        .asks
                        .iter()
                        .take(10)
                        .map(|(price, qty)| trading_cli::OrderbookLevel {
                            price: price.to_string(),
                            quantity: qty.to_string(),
                        })
                        .collect();

                    // Calculate best bid/ask and spread
                    let best_bid = orderbook
                        .best_bid_price()
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                    let best_ask = orderbook
                        .best_ask_price()
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                    let mid_price = orderbook
                        .mid_price()
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                    let spread = if let (Some(bid), Some(ask)) =
                        (orderbook.best_bid_price(), orderbook.best_ask_price())
                    {
                        (ask - bid).to_string()
                    } else {
                        String::new()
                    };

                    (
                        bids,
                        asks,
                        best_bid,
                        best_ask,
                        mid_price,
                        spread,
                        orderbook.timestamp as i64,
                    )
                } else {
                    // No orderbook data yet - return empty
                    (
                        Vec::new(),
                        Vec::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        0,
                    )
                };

            let ob = trading_cli::Orderbook {
                pair: format!("{}/{}", pair.base, pair.quote),
                timestamp,
                bids,
                asks,
                best_bid,
                best_ask,
                mid_price,
                spread,
            };

            debug!(
                "[gRPC] get_orderbooks: Adding orderbook for {}/{}: {} bids, {} asks",
                pair.base,
                pair.quote,
                ob.bids.len(),
                ob.asks.len()
            );

            orderbooks.push(ob);
        }

        debug!(
            "[gRPC] get_orderbooks: Returning {} orderbooks total",
            orderbooks.len()
        );

        // Sort by pair name for consistent ordering
        orderbooks.sort_by(|a, b| a.pair.cmp(&b.pair));

        Ok(orderbooks)
    }
}
