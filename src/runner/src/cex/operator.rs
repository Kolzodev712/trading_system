//! gRPC operator config: persist file + live cap. See design log 012.

use crate::config::runtime_snapshot::{
    write_restart_flag, RuntimeSnapshot, SnapshotStrategy, RESTART_FLAG_FILE, SNAPSHOT_FILE,
};
use crate::config::{StrategiesConfig, StrategyConfig};
use crate::platform::shutdown::ShutdownManager;
use async_trait::async_trait;
use gateway::grpc::service::{OperatorFields, StrategyOperator};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::grpc::TradingControl;
use trading_core::types::common::{InstrumentType, Pair};
use trading_engine::state::manager::TradingStateManager;
use trading_engine::state::{
    flatten_cancel_working_failed, merge_rest_working_orders, submit_operator_reduce_only_exits,
    tsm_working_absent_from_rest, verify_rest_working_orders_cleared, SubmissionJournal,
};
use trading_engine::strategies::strategy_manager::StrategyManager;

struct FlattenPath {
    journal: Arc<SubmissionJournal>,
    trading_state_manager: Arc<TradingStateManager>,
    instrument_type: InstrumentType,
}

pub struct FileStrategyOperator {
    strategies_path: String,
    snapshot_path: String,
    restart_flag_path: String,
    caps: Arc<RwLock<HashMap<String, Decimal>>>,
    strategy_manager: Arc<RwLock<StrategyManager>>,
    trading_control: Arc<TradingControl>,
    shutdown: ShutdownManager,
    flatten_path: Option<FlattenPath>,
}

impl FileStrategyOperator {
    pub fn new(
        strategies_path: String,
        caps: Arc<RwLock<HashMap<String, Decimal>>>,
        strategy_manager: Arc<RwLock<StrategyManager>>,
        trading_control: Arc<TradingControl>,
        shutdown: ShutdownManager,
    ) -> Self {
        Self {
            strategies_path,
            snapshot_path: SNAPSHOT_FILE.to_string(),
            restart_flag_path: RESTART_FLAG_FILE.to_string(),
            caps,
            strategy_manager,
            trading_control,
            shutdown,
            flatten_path: None,
        }
    }

    pub fn with_flatten_path(
        mut self,
        journal: Arc<SubmissionJournal>,
        trading_state_manager: Arc<TradingStateManager>,
        instrument_type: InstrumentType,
    ) -> Self {
        self.flatten_path = Some(FlattenPath {
            journal,
            trading_state_manager,
            instrument_type,
        });
        self
    }

    pub fn with_persist_paths(mut self, snapshot_path: String, restart_flag_path: String) -> Self {
        self.snapshot_path = snapshot_path;
        self.restart_flag_path = restart_flag_path;
        self
    }

    fn parse_decimal(s: &str) -> Result<Decimal, String> {
        s.parse().map_err(|e| format!("invalid decimal '{s}': {e}"))
    }

    fn parse_symbol(symbol: &str) -> Result<Pair, String> {
        let (base, quote) = symbol
            .split_once('/')
            .ok_or_else(|| format!("symbol must be BASE/QUOTE, got {symbol}"))?;
        Ok(Pair::new(base, quote))
    }

    fn to_fields(config: &StrategyConfig) -> OperatorFields {
        OperatorFields {
            strategy_type: config.strategy_type.to_string(),
            symbol: config.symbol.to_string(),
            position_size: config.position_size.to_string(),
            max_position: config.max_position.to_string(),
            buy_threshold_percent: config.buy_threshold_percent.to_string(),
            sell_threshold_percent: config.sell_threshold_percent.to_string(),
            stop_loss_percent: config
                .stop_loss_percent
                .map(|d| d.to_string())
                .unwrap_or_default(),
            take_profit_percent: config
                .take_profit_percent
                .map(|d| d.to_string())
                .unwrap_or_default(),
            max_active_orders: config.max_active_orders as i32,
            price_history_size: config.price_history_size as i32,
            min_price_history: config.min_price_history as i32,
        }
    }
}

#[async_trait]
impl StrategyOperator for FileStrategyOperator {
    async fn get_config(
        &self,
        strategy_name: &str,
        symbol: &str,
    ) -> Result<OperatorFields, String> {
        let pair = Self::parse_symbol(symbol)?;
        let configs =
            StrategiesConfig::load_from_file(&self.strategies_path).map_err(|e| e.to_string())?;
        let found = configs
            .as_vec()
            .into_iter()
            .find(|c| {
                c.strategy_type.as_str() == strategy_name
                    && c.symbol.base == pair.base
                    && c.symbol.quote == pair.quote
            })
            .or_else(|| {
                configs
                    .as_vec()
                    .into_iter()
                    .find(|c| c.symbol.base == pair.base && c.symbol.quote == pair.quote)
            });
        found
            .map(Self::to_fields)
            .ok_or_else(|| format!("no strategy {strategy_name} {symbol}"))
    }

    async fn update_cap(
        &self,
        strategy_name: &str,
        symbol: &str,
        max_position: &str,
    ) -> Result<(), String> {
        let pair = Self::parse_symbol(symbol)?;
        let cap = Self::parse_decimal(max_position)?;
        if cap <= Decimal::ZERO {
            return Err("max_position must be > 0".into());
        }
        let mut configs =
            StrategiesConfig::load_from_file(&self.strategies_path).map_err(|e| e.to_string())?;
        {
            let row = configs
                .find_mut(strategy_name, &pair)
                .ok_or_else(|| format!("no strategy {strategy_name} {symbol}"))?;
            if cap < row.position_size {
                return Err("max_position cannot be below position_size".into());
            }
            row.max_position = cap;
        }
        configs.validate()?;
        configs
            .save_to_file(&self.strategies_path)
            .map_err(|e| e.to_string())?;
        let id = StrategyManager::make_strategy_id(strategy_name, &pair);
        self.caps.write().await.insert(id, cap);
        Ok(())
    }

    async fn update_config(&self, fields: OperatorFields) -> Result<bool, String> {
        let pair = Self::parse_symbol(&fields.symbol)?;
        let mut configs =
            StrategiesConfig::load_from_file(&self.strategies_path).map_err(|e| e.to_string())?;
        let mut needs_restart = false;
        let mut cap_changed = false;
        let mut applied_cap;
        {
            let row = configs
                .find_mut_for_operator(&fields.strategy_type, &pair)
                .ok_or_else(|| format!("no strategy {} {}", fields.strategy_type, fields.symbol))?;
            applied_cap = row.max_position;
            if !fields.strategy_type.is_empty()
                && fields.strategy_type != row.strategy_type.as_str()
            {
                let canonical = canonical_strategy_type(&fields.strategy_type)?;
                row.strategy_type = canonical.into();
                needs_restart = true;
            }
            if !fields.position_size.is_empty() {
                let v = Self::parse_decimal(&fields.position_size)?;
                if v != row.position_size {
                    needs_restart = true;
                    row.position_size = v;
                }
            }
            if !fields.buy_threshold_percent.is_empty() {
                let v = Self::parse_decimal(&fields.buy_threshold_percent)?;
                if v != row.buy_threshold_percent {
                    needs_restart = true;
                    row.buy_threshold_percent = v;
                }
            }
            if !fields.sell_threshold_percent.is_empty() {
                let v = Self::parse_decimal(&fields.sell_threshold_percent)?;
                if v != row.sell_threshold_percent {
                    needs_restart = true;
                    row.sell_threshold_percent = v;
                }
            }
            if !fields.stop_loss_percent.is_empty() {
                let v = Self::parse_decimal(&fields.stop_loss_percent)?;
                if row.stop_loss_percent != Some(v) {
                    needs_restart = true;
                    row.stop_loss_percent = Some(v);
                }
            }
            if !fields.take_profit_percent.is_empty() {
                let v = Self::parse_decimal(&fields.take_profit_percent)?;
                if row.take_profit_percent != Some(v) {
                    needs_restart = true;
                    row.take_profit_percent = Some(v);
                }
            }
            if fields.max_active_orders > 0
                && fields.max_active_orders as usize != row.max_active_orders
            {
                needs_restart = true;
                row.max_active_orders = fields.max_active_orders as usize;
            }
            if fields.price_history_size > 0
                && fields.price_history_size as usize != row.price_history_size
            {
                needs_restart = true;
                row.price_history_size = fields.price_history_size as usize;
            }
            if fields.min_price_history > 0
                && fields.min_price_history as usize != row.min_price_history
            {
                needs_restart = true;
                row.min_price_history = fields.min_price_history as usize;
            }
            if !fields.max_position.is_empty() {
                let v = Self::parse_decimal(&fields.max_position)?;
                if v != row.max_position {
                    row.max_position = v;
                    applied_cap = v;
                    cap_changed = true;
                }
            }
        }

        configs.validate()?;
        configs
            .save_to_file(&self.strategies_path)
            .map_err(|e| e.to_string())?;
        if cap_changed {
            let id = StrategyManager::make_strategy_id(&fields.strategy_type, &pair);
            self.caps.write().await.insert(id, applied_cap);
        }
        Ok(needs_restart)
    }

    async fn request_restart(&self, on_stop: &str) -> Result<(), String> {
        match on_stop {
            "cancel_working" | "flatten" | "keep_working" => {}
            other => {
                return Err(format!(
                    "on_stop={other} is not implemented; use cancel_working, flatten, or keep_working"
                ));
            }
        }
        let manager = self.strategy_manager.read().await;
        let mut strategies = Vec::new();
        for strategy_id in manager.strategy_ids() {
            let enabled = manager.is_strategy_enabled(strategy_id).await;
            strategies.push(SnapshotStrategy {
                strategy_id: strategy_id.clone(),
                enabled,
            });
        }
        drop(manager);
        let snap = RuntimeSnapshot {
            mode: "strategies".into(),
            global_trading: self.trading_control.is_enabled(),
            strategies,
            on_stop: on_stop.to_string(),
        };
        snap.save(&self.snapshot_path)?;
        write_restart_flag(&self.restart_flag_path)?;
        self.trading_control
            .enabled_flag()
            .store(false, std::sync::atomic::Ordering::Release);
        if on_stop == "flatten" {
            self.trading_control.latch_flatten_and_pause();
            let ctx = self.flatten_path.as_ref().ok_or_else(|| {
                "flatten on_stop: journaled reduce-only path required; latch held".to_string()
            })?;
            let (mut order_ids, mut pairs, tsm_positions) = {
                let state = ctx.trading_state_manager.get_state();
                let guard = state.read().await;
                let order_ids = guard
                    .active_orders
                    .values()
                    .map(|order| order.order_id.clone())
                    .collect();
                let mut pairs: Vec<Pair> = guard
                    .active_orders
                    .values()
                    .map(|order| order.instrument.pair.clone())
                    .collect();
                pairs.extend(
                    guard
                        .positions
                        .values()
                        .map(|position| position.symbol.clone()),
                );
                let tsm_positions: Vec<(Pair, Decimal)> = guard
                    .positions
                    .values()
                    .map(|position| (position.symbol.clone(), position.quantity))
                    .collect();
                (order_ids, pairs, tsm_positions)
            };
            let rest_err = {
                let client = self.trading_control.trading_client();
                let client = client.read().await;
                match client.fetch_open_orders().await {
                    Ok(rest) => {
                        merge_rest_working_orders(&mut order_ids, &mut pairs, &rest);
                        None
                    }
                    Err(err) => Some(err.to_string()),
                }
            };
            if let Some(err) = rest_err {
                return Err(flatten_cancel_working_failed(&format!(
                    "REST GET /open_orders failed: {err}"
                )));
            }
            pairs.sort_by(|a, b| a.base.cmp(&b.base).then(a.quote.cmp(&b.quote)));
            pairs.dedup();
            if let Err(e) = self.trading_control.cancel_working(order_ids, pairs).await {
                return Err(flatten_cancel_working_failed(&e.to_string()));
            }
            let rest_after = {
                let client = self.trading_control.trading_client();
                let client = client.read().await;
                match verify_rest_working_orders_cleared(&*client).await {
                    Ok(()) => None,
                    Err(err) => Some(err.to_string()),
                }
            };
            if let Some(err) = rest_after {
                return Err(flatten_cancel_working_failed(&err));
            }
            {
                let state = ctx.trading_state_manager.get_state();
                let guard = state.read().await;
                let stale = tsm_working_absent_from_rest(&guard, &[]);
                if !stale.is_empty() {
                    tracing::warn!(
                        tsm_working_ids = ?stale,
                        "FlattenAndPause CancelWorking: TSM working IDs absent from REST; reconciliation mismatch, not a live operator exit"
                    );
                }
            }
            let client = self.trading_control.trading_client();
            let client = client.read().await;
            let state_lock = ctx.trading_state_manager.get_state();
            let mut state = state_lock.write().await;
            submit_operator_reduce_only_exits(
                ctx.journal.as_ref(),
                &*client,
                &mut state,
                ctx.instrument_type.clone(),
                &tsm_positions,
            )
            .await
            .map_err(|e| format!("flatten on_stop failed: {e}"))?;
        }
        self.shutdown.signal_shutdown();
        Ok(())
    }
}

fn canonical_strategy_type(raw: &str) -> Result<&'static str, String> {
    match raw {
        "PropHold" | "prop_hold" => Ok("PropHold"),
        "Donchian" | "donchian" => Ok("Donchian"),
        "BuyLowSellHigh" | "buy_low_sell_high" => Ok("BuyLowSellHigh"),
        other => Err(format!("unknown strategy type: {other}")),
    }
}
