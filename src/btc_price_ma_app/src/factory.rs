//! Candidate #2 construction. Exchange mode is always rejected.

use crate::adapter::{BtcPriceMaLongFlatV1, BtcPriceMaLongFlatV1Config};
use std::sync::Arc;
use strategy_core::btc_price_ma_long_flat_v1::PRIMARY_LOOKBACK;
use tokio::sync::RwLock;
use trading_core::{error::EngineError, types::ExecutionMode};
use trading_engine::cex::state::Strategy;
use trading_engine::strategies::{ExtraStrategyFactory, StrategyFactoryArgs};

pub struct BtcPriceMaFactory;

impl ExtraStrategyFactory for BtcPriceMaFactory {
    fn try_build(
        &self,
        args: &StrategyFactoryArgs,
    ) -> Option<Result<Arc<RwLock<dyn Strategy>>, EngineError>> {
        if !matches!(
            args.strategy_type.as_str(),
            "BTCPriceMALongFlatV1" | "btc_price_ma_long_flat_v1"
        ) {
            return None;
        }
        if args.execution_mode != ExecutionMode::Shadow {
            return Some(Err(EngineError::new_custom_error(
                "BTCPriceMALongFlatV1 is not a live strategy".into(),
            )));
        }
        Some(
            BtcPriceMaLongFlatV1::new(BtcPriceMaLongFlatV1Config {
                symbol: args.symbol.clone(),
                lookback: args.lookback.unwrap_or(PRIMARY_LOOKBACK),
                instrument_type: args.instrument_type.clone(),
                max_entry_age_secs: args.max_entry_age_secs,
            })
            .map(|adapter| Arc::new(RwLock::new(adapter)) as Arc<RwLock<dyn Strategy>>),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use trading_core::types::common::{Exchange, InstrumentType, Pair};
    use trading_engine::strategies::build_strategy_composed;

    fn args(mode: ExecutionMode) -> StrategyFactoryArgs {
        StrategyFactoryArgs {
            strategy_type: "BTCPriceMALongFlatV1".to_string(),
            symbol: Pair::new("BTC", "USDT"),
            base_position_size: Decimal::new(1, 3),
            max_position: Decimal::new(1, 3),
            base_buy_threshold: Decimal::new(5, 1),
            base_sell_threshold: Decimal::new(5, 1),
            stop_loss_percent: None,
            take_profit_percent: None,
            max_active_orders: 1,
            price_history_size: 30,
            min_price_history: 28,
            asset_service: None,
            exchange: Exchange::Bybit,
            config_min_order_size: Decimal::new(1, 3),
            config_step_size: Decimal::new(1, 3),
            config_tick_size: Decimal::new(1, 1),
            config_min_notional: Decimal::new(5, 0),
            execution_mode: mode,
            instrument_type: InstrumentType::Futures,
            max_entry_age_secs: Some(86_400),
            lookback: Some(28),
        }
    }

    #[test]
    fn default_factory_rejects_candidate_2_even_in_shadow() {
        match trading_engine::strategies::build_strategy(args(ExecutionMode::Shadow)) {
            Err(err) => assert!(
                err.to_string().contains("btc_price_ma_app"),
                "default runner must name the composition binary: {err}"
            ),
            Ok(_) => panic!("default factory must not construct Candidate #2"),
        }
    }

    #[test]
    fn app_factory_builds_in_shadow_and_rejects_exchange() {
        let extra = BtcPriceMaFactory;
        assert!(build_strategy_composed(args(ExecutionMode::Shadow), Some(&extra)).is_ok());
        match build_strategy_composed(args(ExecutionMode::Exchange), Some(&extra)) {
            Err(err) => assert!(err.to_string().contains("not a live strategy")),
            Ok(_) => panic!("Candidate #2 must stay unavailable in Exchange mode"),
        }
    }
}
