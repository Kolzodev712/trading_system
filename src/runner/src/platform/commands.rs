//! Command handler for runner (SetTradingEnabled, GetTradingStatus, ToggleStrategy, level commands).

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_engine::levels::LevelManager;
use trading_engine::strategies::strategy_manager::StrategyManager;
use trading_networking::commands::{Command, CommandHandler, CommandPayload, CommandResponse};

/// Handles commands from Zenoh/gRPC: set trading enabled, toggle strategy, level operations.
pub struct RunnerCommandHandler {
    pub trading_control: Arc<trading_core::grpc::TradingControl>,
    pub trading_state_manager: Arc<trading_engine::state::manager::TradingStateManager>,
    pub exchange_state_manager: Arc<exchange_engine::state::manager::ExchangeStateManager>,
    pub level_manager: Arc<RwLock<LevelManager>>,
    pub strategy_manager: Option<Arc<RwLock<StrategyManager>>>,
}

#[async_trait::async_trait]
impl CommandHandler for RunnerCommandHandler {
    async fn handle(&self, command: Command) -> Result<CommandResponse, String> {
        match command.payload {
            CommandPayload::SetTradingEnabled { enabled } => {
                let mut active_order_ids_set = HashSet::new();
                let mut pairs_with_orders_set = HashSet::new();

                {
                    let state_arc = self.trading_state_manager.get_state();
                    let state = state_arc.read().await;
                    for order in state.active_orders.values() {
                        active_order_ids_set.insert(order.order_id.clone());
                    }
                }

                {
                    let exchange_state_arc = self.exchange_state_manager.get_state();
                    let exchange_state = exchange_state_arc.read().await;
                    for order in exchange_state.get_open_orders() {
                        active_order_ids_set.insert(order.order_id.clone());
                        if let Some(pair) = exchange_state
                            .symbol_registry()
                            .get_pair(&order.instrument.symbol().into())
                        {
                            pairs_with_orders_set.insert(pair.clone());
                        }
                    }
                }

                {
                    let level_manager = self.level_manager.read().await;
                    for level in level_manager.get_all_levels() {
                        for order_id in &level.active_orders {
                            active_order_ids_set.insert(order_id.clone());
                            pairs_with_orders_set.insert(level.pair.clone());
                        }
                    }
                }

                let active_order_ids: Vec<_> = active_order_ids_set.into_iter().collect();
                let pairs_with_orders: Vec<_> = pairs_with_orders_set.into_iter().collect();

                let cancelled = self
                    .trading_control
                    .set_enabled(enabled, active_order_ids, pairs_with_orders)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(CommandResponse {
                    success: true,
                    message: Some(if enabled {
                        "Trading enabled".to_string()
                    } else {
                        format!("Trading disabled. Cancelled {} orders.", cancelled.len())
                    }),
                    data: None,
                })
            }
            CommandPayload::GetTradingStatus => {
                let enabled = self.trading_control.is_enabled();
                Ok(CommandResponse {
                    success: true,
                    message: Some(format!("enabled={}", enabled)),
                    data: None,
                })
            }
            CommandPayload::ToggleStrategy {
                strategy_name,
                symbol,
                enabled,
            } => {
                let Some(ref strategy_manager) = self.strategy_manager else {
                    return Err("Strategy manager not available".to_string());
                };

                let parts: Vec<&str> = symbol.split('/').collect();
                if parts.len() != 2 {
                    return Err("Invalid symbol format; expected BASE/QUOTE".to_string());
                }
                let pair = trading_core::types::common::Pair::new(parts[0], parts[1]);
                let strategy_id = StrategyManager::make_strategy_id(&strategy_name, &pair);

                let global_trading_enabled = self.trading_control.is_enabled();
                if !global_trading_enabled && enabled {
                    return Err(
                        "Cannot enable strategy when global trading is disabled".to_string()
                    );
                }

                let current_enabled = strategy_manager
                    .read()
                    .await
                    .is_strategy_enabled(&strategy_id)
                    .await;
                if current_enabled != enabled {
                    strategy_manager
                        .write()
                        .await
                        .set_strategy_enabled(&strategy_id, enabled)
                        .await;
                }

                Ok(CommandResponse {
                    success: true,
                    message: Some(format!(
                        "strategy {} {}",
                        strategy_id,
                        if enabled { "enabled" } else { "disabled" }
                    )),
                    data: None,
                })
            }
            CommandPayload::GetLevels
            | CommandPayload::GetLevel { .. }
            | CommandPayload::CreateLevel { .. }
            | CommandPayload::UpdateLevel { .. }
            | CommandPayload::DeleteLevel { .. }
            | CommandPayload::ToggleLevel { .. } => Err("command not implemented".to_string()),
        }
    }
}

/// No-op command handler for dex-only mode (Zenoh queryable still needs a handler).
pub struct DexNoopHandler;

#[async_trait::async_trait]
impl CommandHandler for DexNoopHandler {
    async fn handle(&self, _command: Command) -> Result<CommandResponse, String> {
        Ok(CommandResponse {
            success: false,
            message: Some("dex mode: remote commands not supported".to_string()),
            data: None,
        })
    }
}
