//! Build CEX strategies from config `strategy_type`. See design log 008.

use crate::cex::state::Strategy;
use crate::cex::strategies::{
    BtcPriceMaLongFlatV1, BtcPriceMaLongFlatV1Config, BuyLowSellHigh, Donchian, PropHold,
    PRIMARY_LOOKBACK,
};
use asset_engine::AssetService;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use trading_core::{
    error::EngineError,
    types::{
        common::{Exchange, InstrumentType, Pair},
        ExecutionMode,
    },
};

pub struct StrategyFactoryArgs {
    pub strategy_type: String,
    pub symbol: Pair,
    pub base_position_size: Decimal,
    pub max_position: Decimal,
    pub base_buy_threshold: Decimal,
    pub base_sell_threshold: Decimal,
    pub stop_loss_percent: Option<Decimal>,
    pub take_profit_percent: Option<Decimal>,
    pub max_active_orders: usize,
    pub price_history_size: usize,
    pub min_price_history: usize,
    pub asset_service: Option<Arc<AssetService>>,
    pub exchange: Exchange,
    pub config_min_order_size: Decimal,
    pub config_step_size: Decimal,
    pub config_tick_size: Decimal,
    pub config_min_notional: Decimal,
    pub execution_mode: ExecutionMode,
    pub instrument_type: InstrumentType,
    pub max_entry_age_secs: Option<u64>,
    pub lookback: Option<usize>,
}

pub fn build_strategy(args: StrategyFactoryArgs) -> Result<Arc<RwLock<dyn Strategy>>, EngineError> {
    match args.strategy_type.as_str() {
        "PropHold" | "prop_hold" => Ok(Arc::new(RwLock::new(PropHold::new(args.symbol)))),
        "Donchian" | "donchian" => Ok(Arc::new(RwLock::new(Donchian::new(
            args.symbol,
            args.base_position_size,
            args.price_history_size,
            args.min_price_history,
        )))),
        "BuyLowSellHigh" | "buy_low_sell_high" => Ok(Arc::new(RwLock::new(BuyLowSellHigh::new(
            args.symbol,
            args.base_position_size,
            args.max_position,
            args.base_buy_threshold,
            args.base_sell_threshold,
            args.stop_loss_percent,
            args.take_profit_percent,
            args.max_active_orders,
            args.price_history_size,
            args.min_price_history,
            args.asset_service,
            args.exchange,
            args.config_min_order_size,
            args.config_step_size,
            args.config_tick_size,
            args.config_min_notional,
        )))),
        "BTCPriceMALongFlatV1" | "btc_price_ma_long_flat_v1" => {
            if args.execution_mode != ExecutionMode::Shadow {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "BTCPriceMALongFlatV1 is not a live strategy"
                )));
            }
            let adapter = BtcPriceMaLongFlatV1::new(BtcPriceMaLongFlatV1Config {
                symbol: args.symbol,
                lookback: args.lookback.unwrap_or(PRIMARY_LOOKBACK),
                instrument_type: args.instrument_type,
                max_entry_age_secs: args.max_entry_age_secs,
            })?;
            Ok(Arc::new(RwLock::new(adapter)))
        }
        rejected @ ("DonchianDailyV1" | "donchian_daily_v1") => Err(EngineError::CustomError(
            anyhow::anyhow!("{rejected} is not a live strategy"),
        )),
        other => Err(EngineError::CustomError(anyhow::anyhow!(
            "Unknown strategy type: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::types::common::Exchange;

    fn args(strategy_type: &str) -> StrategyFactoryArgs {
        StrategyFactoryArgs {
            strategy_type: strategy_type.to_string(),
            symbol: Pair::new("BTC", "USDT"),
            base_position_size: Decimal::new(1, 3),
            max_position: Decimal::new(1, 3),
            base_buy_threshold: Decimal::new(5, 1),
            base_sell_threshold: Decimal::new(5, 1),
            stop_loss_percent: None,
            take_profit_percent: None,
            max_active_orders: 2,
            price_history_size: 10,
            min_price_history: 2,
            asset_service: None,
            exchange: Exchange::Bybit,
            config_min_order_size: Decimal::new(1, 5),
            config_step_size: Decimal::new(1, 5),
            config_tick_size: Decimal::new(1, 2),
            config_min_notional: Decimal::new(5, 0),
            execution_mode: ExecutionMode::Exchange,
            instrument_type: InstrumentType::Futures,
            max_entry_age_secs: None,
            lookback: None,
        }
    }

    #[test]
    fn builds_prop_hold() {
        assert!(build_strategy(args("PropHold")).is_ok());
    }

    #[test]
    fn builds_donchian() {
        assert!(build_strategy(args("Donchian")).is_ok());
    }

    #[test]
    fn unknown_type_errors() {
        assert!(build_strategy(args("NotAStrategy")).is_err());
    }

    #[test]
    fn donchian_daily_v1_is_not_a_live_strategy() {
        match build_strategy(args("DonchianDailyV1")) {
            Err(err) => assert!(err.to_string().contains("not a live strategy")),
            Ok(_) => panic!("DonchianDailyV1 must not be a live factory strategy"),
        }
    }

    #[test]
    fn btc_price_ma_long_flat_v1_is_not_a_live_strategy() {
        match build_strategy(args("BTCPriceMALongFlatV1")) {
            Err(err) => assert!(err.to_string().contains("not a live strategy")),
            Ok(_) => panic!("BTCPriceMALongFlatV1 must not be a live factory strategy"),
        }
    }

    #[test]
    fn btc_price_ma_long_flat_v1_builds_in_shadow() {
        let mut shadow = args("BTCPriceMALongFlatV1");
        shadow.execution_mode = ExecutionMode::Shadow;
        assert!(build_strategy(shadow).is_ok());
    }
}
