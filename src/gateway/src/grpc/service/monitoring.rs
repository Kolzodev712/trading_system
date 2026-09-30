//! Monitoring data collection and streaming
use crate::grpc::trading_cli;
use exchange_engine::state::manager::ExchangeStateManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::Status;
use tracing::debug;
use trading_core::grpc::TradingControl;
use trading_core::utils::hours_ago;
use trading_engine::dex::{DexIndicatorEngine, DexQuoteManager, DexStrategyManager};
use trading_engine::levels::LevelManager;
use trading_engine::state::manager::TradingStateManager;
use trading_engine::strategies::strategy_manager::StrategyManager;

use super::converters;

fn display_avg_price(quantity: rust_decimal::Decimal, avg_price: rust_decimal::Decimal) -> String {
    if quantity.abs() > rust_decimal::Decimal::new(1, 6)
        && avg_price.abs() <= rust_decimal::Decimal::new(1, 6)
    {
        "?".into()
    } else {
        avg_price.to_string()
    }
}

pub struct MonitoringService {
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub trading_state_manager: Arc<TradingStateManager>,
    pub exchange_state_manager: Arc<ExchangeStateManager>,
    pub dex_state_manager: Option<Arc<dex_engine::state::manager::DexStateManager>>,
    pub dex_quote_manager: Option<Arc<RwLock<DexQuoteManager>>>,
    pub dex_strategy_manager: Option<Arc<RwLock<DexStrategyManager>>>,
    pub dex_indicator_engine: Option<Arc<DexIndicatorEngine>>,
    pub market_data_state_manager: Arc<market_data_engine::state::manager::MarketDataStateManager>,
    pub indicator_calculator: Arc<indicator_engine::calculator::IndicatorCalculator>,
    pub strategy_infos: Vec<StrategyInfo>,
    pub strategy_manager: Option<Arc<RwLock<StrategyManager>>>,
    pub instrument_type: trading_core::types::common::InstrumentType,
    pub exchange: String,
    pub dex_venue_label: Option<String>,
    pub strategy_caps: Option<Arc<RwLock<HashMap<String, rust_decimal::Decimal>>>>,
    pub execution_mode: trading_core::types::ExecutionMode,
    pub venue_label: String,
    pub environment: String,
    pub trading_control: Arc<TradingControl>,
}

#[derive(Clone)]
pub struct StrategyInfo {
    pub strategy_name: String,
    pub symbol: trading_core::types::common::Pair,
    pub max_active_orders: usize,
}

impl MonitoringService {
    pub async fn collect_monitoring_data(&self) -> Result<trading_cli::MonitoringData, Status> {
        let start = std::time::Instant::now();
        debug!(
            "[gRPC] collect_monitoring_data: Starting collection ({} strategy infos)",
            self.strategy_infos.len()
        );
        let ((orders, balances), positions, indicators, trades, orderbooks) = tokio::join!(
            async {
                let exchange_state_arc = self.exchange_state_manager.get_state();
                let exchange_state = exchange_state_arc.read().await;

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
                drop(level_manager);

                let orders: Vec<trading_cli::Order> = exchange_state
                    .get_open_orders()
                    .iter()
                    .filter(|order| {
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
                            .unwrap_or_else(|| (String::new(), "Limit".to_string()));

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

                let balances: Vec<trading_cli::Balance> = exchange_state
                    .get_balances()
                    .iter()
                    .map(|b| trading_cli::Balance {
                        asset: b.symbol.to_string(),
                        free: b.free.to_string(),
                        used: b.used.to_string(),
                        locked: b.used.to_string(),
                        total: b.total.to_string(),
                    })
                    .collect();

                (orders, balances)
            },
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
                            avg_price: display_avg_price(pos.quantity, pos.avg_price),
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

                let trading_state_arc = self.trading_state_manager.get_state();
                let trading_state = trading_state_arc.read().await;
                let trading_positions: Vec<trading_cli::Position> = trading_state
                    .positions
                    .values()
                    .map(|pos| trading_cli::Position {
                        symbol: pos.symbol.convert_to_smolstr().to_string(),
                        quantity: pos.quantity.to_string(),
                        avg_price: pos
                            .cost_basis()
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "?".into()),
                        side: String::new(),
                        mark_price: String::new(),
                        unrealized_pnl: String::new(),
                        leverage: String::new(),
                        margin_used: String::new(),
                        maintenance_margin: String::new(),
                        liquidation_price: String::new(),
                        realized_pnl: String::new(),
                    })
                    .collect();

                // Runner risk state is TradingState. ESM positions are a fallback
                // when TSM has not yet recorded the pair (typical for Futures REST).
                let mut all_positions = exchange_positions.clone();
                for trading_pos in trading_positions.clone() {
                    if let Some(existing) = all_positions
                        .iter_mut()
                        .find(|p| p.symbol == trading_pos.symbol)
                    {
                        *existing = trading_pos;
                    } else {
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
            async { self.get_indicators().await.unwrap_or_default() },
            async { self.get_trades().await.unwrap_or_default() },
            async { self.get_orderbooks().await.unwrap_or_default() }
        );

        let aggregated = trading_cli::AggregatedData {
            overall: Some(trading_cli::OverallStatistics {
                total_orders: orders.len() as i32,
                total_positions: positions.len() as i32,
                total_position_value: String::new(),
                total_pnl: String::new(),
                total_balance: balances
                    .iter()
                    .find(|b| b.asset == "USDT" || b.asset == "USD")
                    .map(|b| b.total.clone())
                    .unwrap_or_default(),
                active_pairs: 1,
                active_levels: 0,
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

        let mut strategy_statuses = Vec::new();
        for strategy_info in &self.strategy_infos {
            match self
                .collect_strategy_status(strategy_info, &positions)
                .await
            {
                Some(status) => strategy_statuses.push(status),
                None => tracing::warn!(
                    "Failed to collect status for strategy {}:{}",
                    strategy_info.strategy_name,
                    strategy_info.symbol
                ),
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

        let instrument_type_str = match self.instrument_type {
            trading_core::types::common::InstrumentType::Spot => "Spot",
            trading_core::types::common::InstrumentType::Futures => "Futures",
            trading_core::types::common::InstrumentType::Derivitives => "Futures",
        };

        let dex = self.collect_dex_snapshot().await;

        let snapshot_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let runner_mode = if self.dex_state_manager.is_some() {
            "dex"
        } else if self.strategy_manager.is_some() {
            "strategies"
        } else {
            "levels"
        };

        let mut data = trading_cli::MonitoringData {
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
            dex,
            execution_mode: self.execution_mode.as_str().to_string(),
            venue: self.venue_label.clone(),
            environment: self.environment.clone(),
            snapshot_unix_ms,
            trading_enabled: self.trading_control.is_enabled().await,
            runner_mode: runner_mode.to_string(),
            entries_paused: self.trading_control.entries_paused().await,
            flatten_latched: self.trading_control.flatten_latched().await,
            global_toggle_semantics: trading_core::grpc::GLOBAL_TOGGLE_SEMANTICS.to_string(),
            economic_source: trading_engine::state::economic_source(
                &self.venue_label,
                &self.environment,
            )
            .to_string(),
            realized_pnl: String::new(),
            unrealized_pnl: String::new(),
            daily_pnl: String::new(),
            fees_paid: String::new(),
            funding_pnl: String::new(),
            ledger_equity: String::new(),
            operator_id: crate::grpc::access::displayed_operator_id(&self.venue_label),
        };

        if let Some(ledger) = self.trading_state_manager.execution_ledger() {
            let marks = self.ledger_marks().await;
            match ledger.snapshot(&marks, snapshot_unix_ms as u64) {
                Ok(snap) => {
                    data.realized_pnl = snap.net_realized.to_string();
                    data.unrealized_pnl = snap.unrealized_pnl.to_string();
                    data.daily_pnl = snap.daily_pnl.to_string();
                    data.fees_paid = snap.fees_paid.to_string();
                    data.funding_pnl = snap.funding_pnl.to_string();
                    data.ledger_equity = snap.equity.to_string();
                    if let Some(overall) = data.aggregated.as_mut().and_then(|a| a.overall.as_mut())
                    {
                        overall.total_pnl = snap.equity.to_string();
                    }
                }
                Err(err) => {
                    tracing::warn!("execution ledger snapshot for monitoring failed: {err}")
                }
            }
        }

        tracing::debug!("[gRPC] collect_monitoring_data: Returning data successfully");
        Ok(data)
    }

    async fn collect_dex_snapshot(&self) -> Option<trading_cli::DexVenueSnapshot> {
        let dex_sm = self.dex_state_manager.as_ref()?;
        let snap = dex_engine::monitoring::collect_dex_snapshot(
            dex_sm,
            self.dex_venue_label
                .clone()
                .unwrap_or_else(|| "dex".to_string()),
        );
        let (quotes, asset_labels) = if let Some(qm) = &self.dex_quote_manager {
            let guard = qm.read().await;
            (
                guard
                    .quote_snapshots()
                    .into_iter()
                    .map(|q| trading_cli::DexQuote {
                        id: q.id,
                        pool: q.pool,
                        side: q.side,
                        price: q.price,
                        quantity: q.quantity,
                        enabled: q.enabled,
                        auto_repost: q.auto_repost,
                        state: q.state,
                        order_id: q.order_id,
                        last_signature: q.last_signature,
                        resolved_price: q.resolved_price,
                    })
                    .collect(),
                guard.asset_labels(),
            )
        } else {
            (Vec::new(), HashMap::new())
        };
        let dex_strategies = if let Some(sm) = &self.dex_strategy_manager {
            let guard = sm.read().await;
            guard
                .status_snapshots()
                .into_iter()
                .map(|s| trading_cli::DexStrategyStatus {
                    id: s.id.as_str().to_string(),
                    strategy_type: s.strategy_type.to_string(),
                    market: s.market.as_str().to_string(),
                    enabled: s.enabled,
                    state: s.state.to_string(),
                    last_signal: s
                        .last_signal
                        .as_ref()
                        .map(|sig| sig.to_string())
                        .unwrap_or_default(),
                    eval_count: s.eval_count,
                })
                .collect()
        } else {
            Vec::new()
        };
        let dex_indicators = if let Some(engine) = &self.dex_indicator_engine {
            engine
                .monitoring_values()
                .await
                .into_iter()
                .map(|v| trading_cli::DexIndicator {
                    indicator_id: v.indicator_id.to_string(),
                    name: v.name.to_string(),
                    pool: v.pool.to_string(),
                    value: v.value.to_string(),
                    timestamp: v.timestamp,
                })
                .collect()
        } else {
            Vec::new()
        };
        Some(trading_cli::DexVenueSnapshot {
            balances: snap
                .balances
                .into_iter()
                .map(|b| trading_cli::DexBalance {
                    mint: b.mint,
                    amount: b.amount,
                })
                .collect(),
            pending_transactions: snap
                .pending_transactions
                .into_iter()
                .map(|p| trading_cli::DexPendingTransaction {
                    signature: p.signature,
                    status: p.status,
                })
                .collect(),
            open_orders: snap
                .open_orders
                .into_iter()
                .map(|o| trading_cli::DexOrder {
                    order_id: o.order_id,
                    pool: o.pool,
                    side: o.side,
                    price: o.price,
                    quantity: o.quantity,
                    status: o.status,
                })
                .collect(),
            venue: snap.venue_label,
            quotes,
            asset_labels,
            dex_strategies,
            dex_indicators,
        })
    }

    pub async fn get_trades(&self) -> Result<Vec<trading_cli::Trade>, Status> {
        let twenty_four_hours_ago = hours_ago(24);

        let state_arc = self.exchange_state_manager.get_state();
        let trades: Vec<trading_core::types::exchange::Trade> = {
            let exchange_state = state_arc.read().await;
            exchange_state
                .get_trades()
                .iter()
                .filter(|t| t.timestamp >= twenty_four_hours_ago)
                .map(|t| (*t).clone())
                .collect()
        };

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

        converted.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
        converted.truncate(100);
        Ok(converted)
    }

    pub async fn get_indicators(&self) -> Result<Vec<trading_cli::IndicatorValue>, Status> {
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

        converted.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
        converted.truncate(100);
        Ok(converted)
    }

    async fn collect_strategy_status(
        &self,
        strategy_info: &StrategyInfo,
        positions: &[trading_cli::Position],
    ) -> Option<trading_cli::StrategyStatus> {
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
        let symbol_no_slash = format!(
            "{}{}",
            strategy_info.symbol.base, strategy_info.symbol.quote
        );
        let position = positions
            .iter()
            .find(|p| p.symbol == symbol_str || p.symbol == symbol_no_slash);

        let trading_state_arc = self.trading_state_manager.get_state();
        let trading_state = trading_state_arc.read().await;
        let active_orders = trading_state
            .active_orders
            .values()
            .filter(|o| {
                let order_symbol: smol_str::SmolStr = o.instrument.symbol().clone();
                order_symbol.as_str() == symbol_str || order_symbol.as_str() == symbol_no_slash
            })
            .count();

        let (position_quantity, position_avg_price, unrealized_pnl, unrealized_pnl_percent) =
            if let Some(pos) = position {
                (
                    pos.quantity.clone(),
                    pos.avg_price.clone(),
                    "N/A".to_string(),
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

        let strategy_id =
            StrategyManager::make_strategy_id(&strategy_info.strategy_name, &strategy_info.symbol);

        let enabled = if let Some(ref strategy_manager) = self.strategy_manager {
            strategy_manager
                .read()
                .await
                .is_strategy_enabled(&strategy_id)
                .await
        } else {
            true
        };

        let snapshot = if let Some(ref strategy_manager) = self.strategy_manager {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            strategy_manager
                .read()
                .await
                .runtime_snapshot(&strategy_id, now_ms)
                .await
        } else {
            None
        };

        let operational_role =
            trading_engine::cex::shadow::operational_role_from_snapshot(snapshot.as_ref());

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
            last_signal: snapshot
                .as_ref()
                .map(|s| s.model_direction.clone())
                .unwrap_or_else(|| "None".to_string()),
            signal_strength: snapshot
                .as_ref()
                .and_then(|s| s.log_spread)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            max_position: if let Some(caps) = &self.strategy_caps {
                caps.read()
                    .await
                    .get(&strategy_id)
                    .map(|d| d.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            },
            execution_mode: self.execution_mode.as_str().to_string(),
            readiness: snapshot
                .as_ref()
                .map(|s| s.readiness.clone())
                .unwrap_or_default(),
            readiness_reason: snapshot
                .as_ref()
                .map(|s| s.readiness_reason.clone())
                .unwrap_or_default(),
            last_completed_candle_start_ms: snapshot
                .as_ref()
                .and_then(|s| s.last_completed_start_ms)
                .unwrap_or(0) as i64,
            last_completed_candle_end_ms: snapshot
                .as_ref()
                .and_then(|s| s.last_completed_end_ms)
                .unwrap_or(0) as i64,
            last_close: snapshot
                .as_ref()
                .and_then(|s| s.last_close)
                .map(|d| d.to_string())
                .unwrap_or_default(),
            geometric_mean: snapshot
                .as_ref()
                .and_then(|s| s.geometric_mean)
                .map(|d| d.to_string())
                .unwrap_or_default(),
            log_spread: snapshot
                .as_ref()
                .and_then(|s| s.log_spread)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            model_direction: snapshot
                .as_ref()
                .map(|s| s.model_direction.clone())
                .unwrap_or_default(),
            desired_target: snapshot
                .as_ref()
                .map(|s| s.desired_target.clone())
                .unwrap_or_default(),
            filled_position: trading_state
                .position_qty(&strategy_info.symbol)
                .to_string(),
            working_qty: trading_state.working_qty(&strategy_info.symbol).to_string(),
            reserved_qty: trading_state
                .reserved_submission_qty(&strategy_info.symbol)
                .to_string(),
            effective_qty: trading_state
                .effective_position_qty(&strategy_info.symbol)
                .to_string(),
            fresh_entry_eligible: snapshot
                .as_ref()
                .map(|s| s.fresh_entry_eligible)
                .unwrap_or(false),
            last_flat_to_long_age_secs: snapshot
                .as_ref()
                .and_then(|s| s.last_flat_to_long_age_secs)
                .map(|age| age as i64)
                .unwrap_or(-1),
            entry_freshness: snapshot
                .as_ref()
                .map(|s| s.entry_freshness.clone())
                .unwrap_or_default(),
            unresolved_submission: trading_state.has_unresolved_submission(&strategy_info.symbol),
            research_disposition: snapshot
                .as_ref()
                .map(|s| s.research_disposition.clone())
                .unwrap_or_default(),
            operational_role,
            strategy_version: snapshot
                .as_ref()
                .map(|s| s.strategy_version.clone())
                .unwrap_or_default(),
            config_hash: snapshot
                .as_ref()
                .map(|s| s.config_hash.clone())
                .unwrap_or_default(),
            previous_direction: snapshot
                .as_ref()
                .map(|s| s.previous_direction.clone())
                .unwrap_or_default(),
            recovery: snapshot
                .as_ref()
                .map(|s| s.recovery.clone())
                .unwrap_or_default(),
            model_reconstructed: snapshot
                .as_ref()
                .map(|s| s.model_reconstructed)
                .unwrap_or(false),
        })
    }

    pub async fn get_orderbooks(&self) -> Result<Vec<trading_cli::Orderbook>, Status> {
        let state_arc = self.market_data_state_manager.get_state();
        let state = state_arc.read().await;

        let symbol_registry = self.market_data_state_manager.symbol_registry();
        let configured_pairs: Vec<trading_core::types::common::Pair> =
            symbol_registry.all_pairs().into_iter().cloned().collect();

        let pairs_with_data = state.get_pairs_with_orderbooks();

        debug!(
            "[gRPC] get_orderbooks: Registry has {} pairs: {:?}, State has {} pairs with data: {:?}",
            configured_pairs.len(),
            configured_pairs
                .iter()
                .map(|p| format!("{}/{}", p.base, p.quote))
                .collect::<Vec<_>>(),
            pairs_with_data.len(),
            pairs_with_data
                .iter()
                .map(|p| format!("{}/{}", p.base, p.quote))
                .collect::<Vec<_>>()
        );

        let pairs = if configured_pairs.is_empty() {
            pairs_with_data
        } else {
            configured_pairs
        };

        let mut orderbooks = Vec::new();

        for pair in pairs {
            let orderbook_opt = state.get_orderbook(&pair);

            let (bids, asks, best_bid, best_ask, mid_price, spread, timestamp) =
                if let Some(orderbook) = orderbook_opt {
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

        orderbooks.sort_by_key(|a| a.pair.clone());
        Ok(orderbooks)
    }

    async fn ledger_marks(
        &self,
    ) -> std::collections::HashMap<trading_core::types::common::Pair, rust_decimal::Decimal> {
        use rust_decimal::Decimal;
        let mut marks = std::collections::HashMap::new();
        let md = self.market_data_state_manager.get_state();
        let state = md.read().await;
        for pair in state.get_pairs_with_orderbooks() {
            if let Some(book) = state.get_orderbook(&pair) {
                let bid = book.bids.first().map(|(price, _)| *price);
                let ask = book.asks.first().map(|(price, _)| *price);
                if let (Some(bid), Some(ask)) = (bid, ask) {
                    marks.insert(pair, (bid + ask) / Decimal::from(2));
                } else if let Some(price) = bid.or(ask) {
                    marks.insert(pair, price);
                }
            }
        }
        marks
    }
}
