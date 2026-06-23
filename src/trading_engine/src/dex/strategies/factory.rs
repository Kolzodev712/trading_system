//! Build `DexStrategy` implementations from config (`type` field).

use std::sync::Arc;

use super::config::{DexStrategiesFile, DexStrategyEntryConfig};
use super::implementations::ObserveMidStrategy;
use super::r#trait::DexStrategy;
use super::types::DexStrategyKind;

type BuiltDexStrategy = (DexStrategyEntryConfig, Arc<dyn DexStrategy>);

pub fn build_strategy(config: &DexStrategyEntryConfig) -> Result<Arc<dyn DexStrategy>, String> {
    match config.strategy_type {
        DexStrategyKind::ObserveMid => Ok(Arc::new(ObserveMidStrategy::new(config.market.clone()))),
        DexStrategyKind::Liquidation => Err(format!(
            "strategy type {} is not configurable via strategies.json (id={})",
            DexStrategyKind::Liquidation,
            config.id.as_str()
        )),
    }
}

pub fn build_from_file(file: DexStrategiesFile) -> Result<Vec<BuiltDexStrategy>, String> {
    let mut out = Vec::with_capacity(file.strategies.len());
    for entry in file.strategies {
        let strategy = build_strategy(&entry)?;
        out.push((entry, strategy));
    }
    Ok(out)
}
