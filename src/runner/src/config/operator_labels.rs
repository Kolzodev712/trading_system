//! Venue / environment / execution-mode labels for operator status.
//! Venue and environment are independent of `ExecutionMode`.

use serde_json::Value as JsonValue;
use trading_core::types::{common::Exchange, ExecutionMode};
use trading_core::venue::VenueMode;

use super::BotConfig;

pub fn execution_status_from_bot(bot: &BotConfig) -> (ExecutionMode, String, String) {
    if bot.venue.mode == VenueMode::Dex {
        return (
            bot.execution_mode,
            "DEEPBOOK".to_string(),
            dex_environment(bot),
        );
    }
    (
        bot.execution_mode,
        venue_label(&bot.exchange_engine.exchange),
        environment_label(&bot.exchange_engine.exchange, &bot.exchange_engine.config),
    )
}

pub fn venue_label(exchange: &Exchange) -> String {
    match exchange {
        Exchange::Bybit => "BYBIT",
        Exchange::Binance => "BINANCE",
        Exchange::Paper => "PAPER",
        Exchange::DeepBook => "DEEPBOOK",
        Exchange::Unsupported => "UNKNOWN",
    }
    .to_string()
}

/// Bybit DEMO is only the dedicated demo host (`demo: true`), never inferred from `testnet`.
pub fn environment_label(exchange: &Exchange, config: &JsonValue) -> String {
    match exchange {
        Exchange::Paper => "LOCAL".to_string(),
        Exchange::Bybit => {
            if config.get("demo").and_then(|v| v.as_bool()) == Some(true) {
                "DEMO".to_string()
            } else if config.get("testnet").and_then(|v| v.as_bool()) == Some(true) {
                "TESTNET".to_string()
            } else {
                "MAINNET".to_string()
            }
        }
        Exchange::Binance => {
            if config.get("testnet").and_then(|v| v.as_bool()) == Some(true) {
                "TESTNET".to_string()
            } else {
                "MAINNET".to_string()
            }
        }
        Exchange::DeepBook | Exchange::Unsupported => "UNKNOWN".to_string(),
    }
}

fn dex_environment(bot: &BotConfig) -> String {
    let Some(dex) = bot.dex_engine.as_ref() else {
        return "UNKNOWN".to_string();
    };
    match dex.config.get("network").and_then(|v| v.as_str()) {
        Some("mainnet") => "MAINNET".to_string(),
        Some("testnet") => "TESTNET".to_string(),
        Some("devnet") => "DEVNET".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trading_core::types::common::Exchange;

    #[test]
    fn paper_is_local() {
        assert_eq!(environment_label(&Exchange::Paper, &json!({})), "LOCAL");
        assert_eq!(venue_label(&Exchange::Paper), "PAPER");
    }

    #[test]
    fn bybit_testnet_is_not_demo() {
        assert_eq!(
            environment_label(&Exchange::Bybit, &json!({"testnet": true, "demo": false})),
            "TESTNET"
        );
    }

    #[test]
    fn bybit_demo_flag_selects_demo() {
        assert_eq!(
            environment_label(&Exchange::Bybit, &json!({"testnet": true, "demo": true})),
            "DEMO"
        );
        assert_eq!(
            environment_label(&Exchange::Bybit, &json!({"demo": true})),
            "DEMO"
        );
    }

    #[test]
    fn bybit_mainnet_when_neither_flag() {
        assert_eq!(
            environment_label(&Exchange::Bybit, &json!({"testnet": false})),
            "MAINNET"
        );
    }
}
