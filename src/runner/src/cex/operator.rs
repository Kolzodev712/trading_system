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
use trading_core::types::common::Pair;
use trading_engine::strategies::strategy_manager::StrategyManager;

pub struct FileStrategyOperator {
    strategies_path: String,
    snapshot_path: String,
    restart_flag_path: String,
    caps: Arc<RwLock<HashMap<String, Decimal>>>,
    strategy_manager: Arc<RwLock<StrategyManager>>,
    trading_control: Arc<TradingControl>,
    shutdown: ShutdownManager,
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
        }
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
        let found = configs.as_vec().into_iter().find(|c| {
            c.strategy_type.as_str() == strategy_name
                && c.symbol.base == pair.base
                && c.symbol.quote == pair.quote
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
                .find_mut(&fields.strategy_type, &pair)
                .ok_or_else(|| format!("no strategy {} {}", fields.strategy_type, fields.symbol))?;
            applied_cap = row.max_position;
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
        if on_stop != "cancel_working" {
            return Err(format!(
                "on_stop={on_stop} is not implemented; use cancel_working"
            ));
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
            global_trading: self.trading_control.is_enabled().await,
            strategies,
            on_stop: on_stop.to_string(),
        };
        snap.save(&self.snapshot_path)?;
        write_restart_flag(&self.restart_flag_path)?;
        *self.trading_control.enabled_flag().write().await = false;
        self.shutdown.signal_shutdown();
        Ok(())
    }
}
