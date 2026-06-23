//! In-memory DEX strategy registry, evaluation cooldowns, and status snapshots.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dex_engine::state::DexState;
use trading_core::channels::market_data::MarketDataUpdate;
use trading_core::venue::dex::TradeCommand;

use super::config::{DexStrategiesFile, DexStrategyDefaults};
use super::factory::build_from_file;
use super::r#trait::{DexStrategy, DexStrategyOutcome};
use super::runtime::{DexStrategyRuntime, DexStrategyRuntimeState};
use super::types::{
    DexMarket, DexObserveSignal, DexRuntimeStateLabel, DexStrategyId, DexStrategyKind,
};

const MIN_FAILURE_BACKOFF_SECS: u64 = 30;

/// Read-only row for gRPC / TUI monitoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DexStrategyStatusSnapshot {
    pub id: DexStrategyId,
    pub strategy_type: DexStrategyKind,
    pub market: DexMarket,
    pub enabled: bool,
    pub state: DexRuntimeStateLabel,
    pub last_signal: Option<DexObserveSignal>,
    pub eval_count: u64,
}

struct ManagedStrategy {
    runtime: DexStrategyRuntime,
    strategy: Arc<dyn DexStrategy>,
}

pub struct DexStrategyManager {
    defaults: DexStrategyDefaults,
    strategies: HashMap<DexStrategyId, ManagedStrategy>,
}

impl DexStrategyManager {
    pub fn from_file(file: DexStrategiesFile) -> Result<Self, String> {
        let defaults = file.defaults.clone();
        let built = build_from_file(file)?;
        let mut strategies = HashMap::new();
        for (config, strategy) in built {
            if strategies.contains_key(&config.id) {
                return Err(format!("duplicate DEX strategy id {}", config.id.as_str()));
            }
            strategies.insert(
                config.id.clone(),
                ManagedStrategy {
                    runtime: DexStrategyRuntime::new(config),
                    strategy,
                },
            );
        }
        Ok(Self {
            defaults,
            strategies,
        })
    }

    pub fn strategy_count(&self) -> usize {
        self.strategies.len()
    }

    pub fn status_snapshots(&self) -> Vec<DexStrategyStatusSnapshot> {
        let mut out: Vec<DexStrategyStatusSnapshot> = self
            .strategies
            .values()
            .map(|entry| DexStrategyStatusSnapshot {
                id: entry.runtime.config.id.clone(),
                strategy_type: entry.strategy.kind(),
                market: entry.runtime.config.market.clone(),
                enabled: entry.runtime.config.enabled,
                state: DexRuntimeStateLabel::from(&entry.runtime.state),
                last_signal: entry.runtime.last_signal.clone(),
                eval_count: entry.runtime.eval_count,
            })
            .collect();
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        out
    }

    pub fn toggle_strategy(&mut self, strategy_id: &str, enabled: bool) -> Result<(), String> {
        let id = DexStrategyId::from(strategy_id);
        let entry = self
            .strategies
            .get_mut(&id)
            .ok_or_else(|| format!("unknown DEX strategy id {strategy_id}"))?;
        entry.runtime.config.enabled = enabled;
        if enabled {
            if matches!(entry.runtime.state, DexStrategyRuntimeState::Disabled) {
                entry.runtime.state = DexStrategyRuntimeState::Idle;
            }
        } else {
            entry.runtime.state = DexStrategyRuntimeState::Disabled;
        }
        Ok(())
    }

    /// Evaluate all eligible strategies; returns trade commands to submit.
    pub fn evaluate(
        &mut self,
        market_data: &MarketDataUpdate,
        dex_state: &DexState,
        now: Instant,
    ) -> Vec<(DexStrategyId, TradeCommand)> {
        let mut commands = Vec::new();
        for (id, entry) in &mut self.strategies {
            if !entry.runtime.can_evaluate(now) {
                continue;
            }
            match entry.strategy.evaluate(market_data, dex_state) {
                DexStrategyOutcome::None => {}
                DexStrategyOutcome::Observe(signal) => {
                    entry.runtime.last_signal = Some(signal);
                    entry.runtime.eval_count += 1;
                    entry.runtime.last_eval_at = Some(now);
                    Self::schedule_eval_cooldown(entry, now, &self.defaults);
                }
                DexStrategyOutcome::Command(cmd) => {
                    entry.runtime.eval_count += 1;
                    entry.runtime.last_eval_at = Some(now);
                    Self::schedule_eval_cooldown(entry, now, &self.defaults);
                    commands.push((id.clone(), cmd));
                }
            }
        }
        commands
    }

    pub fn on_execute_failed(&mut self, strategy_id: &str, now: Instant) {
        let id = DexStrategyId::from(strategy_id);
        let Some(entry) = self.strategies.get_mut(&id) else {
            return;
        };
        let delay = Duration::from_secs(
            self.defaults
                .failure_backoff_secs
                .max(MIN_FAILURE_BACKOFF_SECS),
        );
        entry.runtime.state = DexStrategyRuntimeState::FailureBackoff { after: now + delay };
    }

    fn schedule_eval_cooldown(
        entry: &mut ManagedStrategy,
        now: Instant,
        defaults: &DexStrategyDefaults,
    ) {
        let delay = Duration::from_secs(defaults.eval_cooldown_secs);
        entry.runtime.state = DexStrategyRuntimeState::EvalCooldown { after: now + delay };
    }
}

impl fmt::Display for DexStrategyStatusSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} enabled={} state={}",
            self.id.as_str(),
            self.strategy_type,
            self.market.as_str(),
            self.enabled,
            self.state
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex::strategies::config::DexStrategyEntryConfig;
    use smol_str::SmolStr;

    fn sample_file() -> DexStrategiesFile {
        DexStrategiesFile {
            strategies: vec![DexStrategyEntryConfig {
                id: DexStrategyId::from("observe_mid_sui_dbusdc"),
                strategy_type: DexStrategyKind::ObserveMid,
                market: DexMarket(SmolStr::new("SUI_DBUSDC")),
                enabled: true,
            }],
            defaults: DexStrategyDefaults::default(),
        }
    }

    #[test]
    fn loads_observe_mid_strategy() {
        let mgr = DexStrategyManager::from_file(sample_file()).unwrap();
        assert_eq!(mgr.strategy_count(), 1);
    }

    #[test]
    fn toggle_disables_strategy() {
        let mut mgr = DexStrategyManager::from_file(sample_file()).unwrap();
        mgr.toggle_strategy("observe_mid_sui_dbusdc", false)
            .unwrap();
        let snap = mgr.status_snapshots().into_iter().next().unwrap();
        assert!(!snap.enabled);
        assert_eq!(snap.state, DexRuntimeStateLabel::Disabled);
    }
}
