//! Canonical expanded level identity.
use smol_str::SmolStr;
use trading_core::types::common::{Exchange, InstrumentType, Pair};

/// Unambiguous level instance: venue, account, instrument, pair, template, expanded id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelIdentity {
    pub venue: Exchange,
    pub instrument: InstrumentType,
    pub account: SmolStr,
    pub pair: Pair,
    pub template_id: SmolStr,
    pub instance_id: SmolStr,
}

impl LevelIdentity {
    pub fn new(
        venue: Exchange,
        instrument: InstrumentType,
        account: impl Into<SmolStr>,
        pair: Pair,
        template_id: impl Into<SmolStr>,
    ) -> Self {
        let template_id = template_id.into();
        let instance_id = expanded_level_id(&template_id, &pair);
        Self {
            venue,
            instrument,
            account: account.into(),
            pair,
            template_id,
            instance_id,
        }
    }

    pub fn for_tests(pair: Pair, template_id: impl Into<SmolStr>) -> Self {
        Self::new(
            Exchange::Paper,
            InstrumentType::Spot,
            SmolStr::new("test"),
            pair,
            template_id,
        )
    }
}

/// Pair-specific instance id: `{template}_{BASE}{QUOTE}` (e.g. `buy_level_1_BTCUSDT`).
pub fn expanded_level_id(template_id: &str, pair: &Pair) -> SmolStr {
    SmolStr::new(format!("{}_{}", template_id, pair.convert_to_smolstr()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expanded_id_is_pair_specific() {
        let btc = Pair::new("BTC", "USDT");
        let eth = Pair::new("ETH", "USDT");
        assert_eq!(
            expanded_level_id("buy_level_1", &btc).as_str(),
            "buy_level_1_BTCUSDT"
        );
        assert_eq!(
            expanded_level_id("buy_level_1", &eth).as_str(),
            "buy_level_1_ETHUSDT"
        );
        assert_ne!(
            expanded_level_id("buy_level_1", &btc),
            expanded_level_id("buy_level_1", &eth)
        );
    }
}
