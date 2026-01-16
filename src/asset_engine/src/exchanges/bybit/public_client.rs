//! Bybit public REST client for fetching exchange information (unsigned calls)
use super::types::BybitInstrumentsInfoResponse;
use crate::service::AssetDataClient;
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    types::{
        asset::{ExchangeLimits, ExchangeMarketData, SymbolInfo, SymbolStatus, TradingRules},
        common::Pair,
    },
};

/// Bybit public REST client (no authentication required)
#[derive(Debug)]
pub struct BybitPublicClient {
    base_url: String,
    http_client: Client,
}

impl BybitPublicClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http_client: Client::new(),
        }
    }

    /// Fetch instruments info for a specific category (spot, linear, or inverse)
    /// Returns all instruments for the category (handles pagination internally)
    async fn fetch_instruments_info(
        &self,
        category: &str,
    ) -> Result<BybitInstrumentsInfoResponse, EngineError> {
        let mut all_instruments = Vec::new();
        let mut cursor = String::new();
        let category_clone = category.to_string();

        loop {
            let url = format!(
                "{}/v5/market/instruments-info?category={}&limit=1000{}",
                self.base_url,
                category,
                if cursor.is_empty() {
                    String::new()
                } else {
                    // Simple URL encoding for cursor parameter
                    let encoded: String = cursor
                        .chars()
                        .map(|c| match c {
                            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                                c.to_string()
                            }
                            _ => format!("%{:02X}", c as u8),
                        })
                        .collect();
                    format!("&cursor={}", encoded)
                }
            );

            info!(
                "Fetching instruments info from {} (category: {})",
                url, category
            );

            let response = self
                .http_client
                .get(&url)
                .send()
                .await
                .map_err(|e| EngineError::CustomError(anyhow::anyhow!("HTTP error: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                error!("Instruments info request failed: {} - {}", status, text);
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "Instruments info request failed: {} - {}",
                    status,
                    text
                )));
            }

            let mut instruments_response: BybitInstrumentsInfoResponse =
                response.json().await.map_err(|e| {
                    EngineError::CustomError(anyhow::anyhow!("JSON parse error: {}", e))
                })?;

            if instruments_response.ret_code != 0 {
                return Err(EngineError::CustomError(anyhow::anyhow!(
                    "Bybit API error: {} - {}",
                    instruments_response.ret_code,
                    instruments_response.ret_msg
                )));
            }

            // Collect instruments from this page
            all_instruments.append(&mut instruments_response.result.list);

            // Check if there's a next page
            cursor = instruments_response.result.next_page_cursor.clone();
            if cursor.is_empty() {
                break;
            }

            info!(
                "Fetched page: {} instruments, cursor: {}",
                instruments_response.result.list.len(),
                cursor
            );
        }

        info!(
            "Fetched total {} instruments for category: {}",
            all_instruments.len(),
            category
        );

        // Build final response with all instruments
        Ok(BybitInstrumentsInfoResponse {
            ret_code: 0,
            ret_msg: "OK".to_string(),
            result: super::types::BybitInstrumentsInfoResult {
                category: category_clone,
                next_page_cursor: String::new(),
                list: all_instruments,
            },
            time: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Fetch instruments info for both spot and futures (linear/inverse)
    /// Returns combined response with all instruments
    async fn fetch_all_instruments_info(
        &self,
    ) -> Result<Vec<BybitInstrumentsInfoResponse>, EngineError> {
        let mut results = Vec::new();

        // Fetch spot instruments
        let spot_result = self.fetch_instruments_info("spot").await?;
        results.push(spot_result);

        // Fetch linear (USDT perpetual) instruments
        let linear_result = self.fetch_instruments_info("linear").await?;
        results.push(linear_result);

        // Fetch inverse (coin-margined) instruments
        let inverse_result = self.fetch_instruments_info("inverse").await?;
        results.push(inverse_result);

        Ok(results)
    }

    /// Parse Bybit exchange info into common types
    fn parse_exchange_info(
        bybit_responses: &[BybitInstrumentsInfoResponse],
        pairs: &[Pair],
    ) -> Result<(HashMap<Pair, SymbolInfo>, HashMap<Pair, TradingRules>), EngineError> {
        let mut symbol_info_map = HashMap::new();
        let mut trading_rules_map = HashMap::new();

        // Create a set of pairs we're interested in (for filtering)
        let pair_set: std::collections::HashSet<_> = pairs.iter().collect();

        for response in bybit_responses {
            let category = &response.result.category;
            for instrument in &response.result.list {
                // Convert to Pair
                let pair = Pair::new(
                    instrument.base_coin.as_str(),
                    instrument.quote_coin.as_str(),
                );

                // Only process pairs we're interested in
                if !pair_set.contains(&pair) {
                    continue;
                }

                // Parse symbol status
                let status = match instrument.status.as_ref() {
                    "Trading" => SymbolStatus::Trading,
                    "PreLaunch" => SymbolStatus::PreTrading,
                    "Settling" => SymbolStatus::PostTrading,
                    "Delivering" => SymbolStatus::EndOfDay,
                    "Closed" => SymbolStatus::Halt,
                    _ => SymbolStatus::Halt, // Default to halt for unknown status
                };

                let trading_allowed = status == SymbolStatus::Trading;
                // For spot, check if margin trading is enabled; for futures, always active if trading
                let is_active = trading_allowed
                    && (category == "spot" || category == "linear" || category == "inverse");

                // Create symbol info
                let symbol_info = SymbolInfo {
                    pair: pair.clone(),
                    status,
                    base_asset: instrument.base_coin.clone(),
                    quote_asset: instrument.quote_coin.clone(),
                    exchange_symbol: instrument.symbol.clone(),
                    trading_allowed,
                    is_active,
                };

                // Parse trading rules
                let trading_rules = Self::parse_trading_rules(instrument, &pair, category)?;

                // Insert both
                symbol_info_map.insert(pair.clone(), symbol_info);
                trading_rules_map.insert(pair, trading_rules);
            }
        }

        Ok((symbol_info_map, trading_rules_map))
    }

    /// Parse trading rules from Bybit instrument info
    fn parse_trading_rules(
        instrument: &super::types::BybitInstrumentInfo,
        pair: &Pair,
        category: &str,
    ) -> Result<TradingRules, EngineError> {
        // Extract values from lot size filter
        let min_order_size = Some(instrument.lot_size_filter.min_order_qty);
        let max_order_size = Some(instrument.lot_size_filter.max_order_qty);
        // qty_step is only available for linear/inverse, not for spot
        // For spot, derive step_size from base_precision if qty_step is not available
        let step_size = if category == "spot" {
            instrument
                .lot_size_filter
                .qty_step
                .or_else(|| instrument.lot_size_filter.base_precision)
        } else {
            instrument.lot_size_filter.qty_step
        };

        // Extract values from price filter
        let min_price = instrument.price_filter.min_price;
        let max_price = instrument.price_filter.max_price;
        let tick_size = Some(instrument.price_filter.tick_size);

        // Min notional - different fields for spot vs futures
        let min_notional = if category == "spot" {
            instrument.lot_size_filter.min_order_amt
        } else {
            instrument.lot_size_filter.min_notional_value
        };

        // Precision - for spot, use base/quote precision; for futures, derive from price_scale
        let price_precision = if category == "spot" {
            instrument
                .lot_size_filter
                .quote_precision
                .and_then(|precision_decimal| {
                    // Convert Decimal to u8 precision
                    let precision_string = precision_decimal.to_string();
                    if precision_string.contains('.') {
                        precision_string.split('.').nth(1).and_then(|decimal_part| {
                            decimal_part.trim_end_matches('0').len().try_into().ok()
                        })
                    } else {
                        Some(0)
                    }
                })
        } else {
            instrument
                .price_scale
                .as_ref()
                .and_then(|scale_string| scale_string.parse::<u8>().ok())
        };

        let quantity_precision = if category == "spot" {
            instrument
                .lot_size_filter
                .base_precision
                .and_then(|precision_decimal| {
                    let precision_string = precision_decimal.to_string();
                    if precision_string.contains('.') {
                        precision_string.split('.').nth(1).and_then(|decimal_part| {
                            decimal_part.trim_end_matches('0').len().try_into().ok()
                        })
                    } else {
                        Some(0)
                    }
                })
        } else {
            // For futures, derive from qty_step (should always be present for linear/inverse)
            instrument.lot_size_filter.qty_step.map(|qty_step_decimal| {
                let quantity_step_string = qty_step_decimal.to_string();
                if quantity_step_string.contains('.') {
                    quantity_step_string
                        .split('.')
                        .nth(1)
                        .and_then(|decimal_part| {
                            decimal_part.trim_end_matches('0').len().try_into().ok()
                        })
                        .unwrap_or(0)
                } else {
                    0
                }
            })
        };

        Ok(TradingRules {
            pair: pair.clone(),
            min_order_size,
            max_order_size,
            step_size,
            min_price,
            max_price,
            tick_size,
            min_notional,
            max_notional: if category == "spot" {
                instrument.lot_size_filter.max_order_amt
            } else {
                None
            },
            price_precision,
            quantity_precision,
        })
    }

    /// Parse Bybit exchange limits (minimal - rate limits not in instruments-info endpoint)
    fn parse_exchange_limits() -> ExchangeLimits {
        ExchangeLimits {
            rate_limits: Vec::new(), // Rate limits not provided by instruments-info endpoint
            max_position_size: None,
            max_open_orders: None,
            other_limits: HashMap::new(),
        }
    }
}

// Implement AssetDataClient for BybitPublicClient
#[async_trait]
impl AssetDataClient for BybitPublicClient {
    async fn fetch_market_data(&self, pairs: &[Pair]) -> Result<ExchangeMarketData, EngineError> {
        let bybit_responses = self.fetch_all_instruments_info().await?;

        // Parse exchange info into common types
        let (symbol_info_map, trading_rules_map) =
            Self::parse_exchange_info(&bybit_responses, pairs)?;
        let exchange_limits = Self::parse_exchange_limits();

        // Get timestamp from first response (or current time if empty)
        let last_updated = bybit_responses
            .first()
            .map(|response| response.time)
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);

        // Build market data
        let market_data = ExchangeMarketData {
            symbol_info: symbol_info_map,
            trading_rules: trading_rules_map,
            current_prices: HashMap::new(), // Will be populated by separate call if needed
            exchange_limits,
            last_updated,
        };

        Ok(market_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchanges::bybit::types::{
        BybitInstrumentInfo, BybitInstrumentsInfoResponse, BybitInstrumentsInfoResult,
        BybitLotSizeFilter, BybitPriceFilter,
    };
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;
    use trading_core::types::asset::SymbolStatus;

    fn create_test_bybit_spot_instrument() -> BybitInstrumentInfo {
        BybitInstrumentInfo {
            symbol: SmolStr::new("BTCUSDT"),
            status: SmolStr::new("Trading"),
            base_coin: SmolStr::new("BTC"),
            quote_coin: SmolStr::new("USDT"),
            margin_trading: Some(SmolStr::new("utaOnly")),
            contract_type: None,
            lot_size_filter: BybitLotSizeFilter {
                base_precision: Some(dec!(0.000001)),
                quote_precision: Some(dec!(0.0000001)),
                min_order_amt: Some(dec!(5)),
                max_order_amt: Some(dec!(8000000)),
                min_order_qty: dec!(0.000011),
                max_order_qty: dec!(83),
                qty_step: None, // Not present for spot
                min_notional_value: None,
                max_limit_order_qty: Some(dec!(83)),
                max_market_order_qty: Some(dec!(41.5)),
            },
            price_filter: BybitPriceFilter {
                min_price: None,
                max_price: None,
                tick_size: dec!(0.1),
            },
            price_scale: None,
        }
    }

    fn create_test_bybit_linear_instrument() -> BybitInstrumentInfo {
        BybitInstrumentInfo {
            symbol: SmolStr::new("BTCUSDT"),
            status: SmolStr::new("Trading"),
            base_coin: SmolStr::new("BTC"),
            quote_coin: SmolStr::new("USDT"),
            margin_trading: None,
            contract_type: Some(SmolStr::new("LinearPerpetual")),
            lot_size_filter: BybitLotSizeFilter {
                base_precision: None,
                quote_precision: None,
                min_order_amt: None,
                max_order_amt: None,
                min_order_qty: dec!(0.001),
                max_order_qty: dec!(1190.000),
                qty_step: Some(dec!(0.001)), // Present for linear/inverse
                min_notional_value: Some(dec!(5)),
                max_limit_order_qty: None,
                max_market_order_qty: None,
            },
            price_filter: BybitPriceFilter {
                min_price: Some(dec!(0.10)),
                max_price: Some(dec!(1999999.80)),
                tick_size: dec!(0.10),
            },
            price_scale: Some("2".to_string()),
        }
    }

    fn create_test_bybit_spot_response() -> BybitInstrumentsInfoResponse {
        BybitInstrumentsInfoResponse {
            ret_code: 0,
            ret_msg: "OK".to_string(),
            result: BybitInstrumentsInfoResult {
                category: "spot".to_string(),
                next_page_cursor: String::new(),
                list: vec![create_test_bybit_spot_instrument()],
            },
            time: 1734819696263,
        }
    }

    fn create_test_bybit_linear_response() -> BybitInstrumentsInfoResponse {
        BybitInstrumentsInfoResponse {
            ret_code: 0,
            ret_msg: "OK".to_string(),
            result: BybitInstrumentsInfoResult {
                category: "linear".to_string(),
                next_page_cursor: String::new(),
                list: vec![create_test_bybit_linear_instrument()],
            },
            time: 1734819696263,
        }
    }

    #[test]
    fn test_parse_exchange_info_filters_by_pairs() {
        let bybit_responses = vec![create_test_bybit_spot_response()];
        let requested_pairs = vec![Pair::new("BTC", "USDT")];

        let (symbol_info_map, trading_rules_map) =
            BybitPublicClient::parse_exchange_info(&bybit_responses, &requested_pairs).unwrap();

        assert_eq!(symbol_info_map.len(), 1);
        assert_eq!(trading_rules_map.len(), 1);
        assert!(symbol_info_map.contains_key(&Pair::new("BTC", "USDT")));
    }

    #[test]
    fn test_parse_exchange_info_handles_multiple_categories() {
        // Create responses with different pairs to avoid overwriting
        let spot_response = create_test_bybit_spot_response();
        let mut linear_response = create_test_bybit_linear_response();

        // Change linear to use ETHUSDT
        linear_response.result.list[0].symbol = SmolStr::new("ETHUSDT");
        linear_response.result.list[0].base_coin = SmolStr::new("ETH");

        let bybit_responses = vec![spot_response, linear_response];
        let requested_pairs = vec![Pair::new("BTC", "USDT"), Pair::new("ETH", "USDT")];

        let (symbol_info_map, trading_rules_map) =
            BybitPublicClient::parse_exchange_info(&bybit_responses, &requested_pairs).unwrap();

        // Should have both spot BTCUSDT and linear ETHUSDT entries
        assert_eq!(symbol_info_map.len(), 2);
        assert_eq!(trading_rules_map.len(), 2);
        assert!(symbol_info_map.contains_key(&Pair::new("BTC", "USDT")));
        assert!(symbol_info_map.contains_key(&Pair::new("ETH", "USDT")));
    }

    #[test]
    fn test_parse_exchange_info_symbol_status_mapping() {
        let mut response = create_test_bybit_spot_response();
        response.result.list[0].status = SmolStr::new("Closed");

        let bybit_responses = vec![response];
        let requested_pairs = vec![Pair::new("BTC", "USDT")];

        let (symbol_info_map, _) =
            BybitPublicClient::parse_exchange_info(&bybit_responses, &requested_pairs).unwrap();

        let symbol_info = symbol_info_map.get(&Pair::new("BTC", "USDT")).unwrap();
        assert_eq!(symbol_info.status, SymbolStatus::Halt);
        assert!(!symbol_info.trading_allowed);
    }

    #[test]
    fn test_parse_trading_rules_spot_instrument() {
        let instrument = create_test_bybit_spot_instrument();
        let pair = Pair::new("BTC", "USDT");

        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "spot").unwrap();

        assert_eq!(trading_rules.pair, pair);
        assert_eq!(trading_rules.min_order_size, Some(dec!(0.000011)));
        assert_eq!(trading_rules.max_order_size, Some(dec!(83)));
        assert_eq!(trading_rules.step_size, Some(dec!(0.000001)));
        assert_eq!(trading_rules.tick_size, Some(dec!(0.1)));
        assert_eq!(trading_rules.min_notional, Some(dec!(5)));
        assert_eq!(trading_rules.max_notional, Some(dec!(8000000)));
        // Precision should be derived from base/quote precision
        assert!(trading_rules.price_precision.is_some());
        assert!(trading_rules.quantity_precision.is_some());
    }

    #[test]
    fn test_parse_trading_rules_linear_instrument() {
        let instrument = create_test_bybit_linear_instrument();
        let pair = Pair::new("BTC", "USDT");

        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "linear").unwrap();

        assert_eq!(trading_rules.pair, pair);
        assert_eq!(trading_rules.min_order_size, Some(dec!(0.001)));
        assert_eq!(trading_rules.max_order_size, Some(dec!(1190.000)));
        assert_eq!(trading_rules.step_size, Some(dec!(0.001)));
        assert_eq!(trading_rules.min_price, Some(dec!(0.10)));
        assert_eq!(trading_rules.max_price, Some(dec!(1999999.80)));
        assert_eq!(trading_rules.tick_size, Some(dec!(0.10)));
        assert_eq!(trading_rules.min_notional, Some(dec!(5)));
        assert_eq!(trading_rules.max_notional, None); // Not available for futures
                                                      // Price precision should come from price_scale
        assert_eq!(trading_rules.price_precision, Some(2));
        // Quantity precision should be derived from qty_step
        assert!(trading_rules.quantity_precision.is_some());
    }

    #[test]
    fn test_parse_trading_rules_precision_calculation_spot() {
        let mut instrument = create_test_bybit_spot_instrument();
        instrument.lot_size_filter.base_precision = Some(dec!(0.0001));
        instrument.lot_size_filter.quote_precision = Some(dec!(0.01));

        let pair = Pair::new("BTC", "USDT");
        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "spot").unwrap();

        // base_precision 0.0001 = 4 decimal places
        assert_eq!(trading_rules.quantity_precision, Some(4));
        // quote_precision 0.01 = 2 decimal places
        assert_eq!(trading_rules.price_precision, Some(2));
    }

    #[test]
    fn test_parse_trading_rules_precision_calculation_futures() {
        let mut instrument = create_test_bybit_linear_instrument();
        instrument.price_scale = Some("4".to_string());
        instrument.lot_size_filter.qty_step = Some(dec!(0.0001));

        let pair = Pair::new("BTC", "USDT");
        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "linear").unwrap();

        // price_scale "4" = 4 decimal places
        assert_eq!(trading_rules.price_precision, Some(4));
        // qty_step 0.0001 = 4 decimal places
        assert_eq!(trading_rules.quantity_precision, Some(4));
    }

    #[test]
    fn test_parse_trading_rules_handles_missing_precision_fields() {
        let mut instrument = create_test_bybit_spot_instrument();
        instrument.lot_size_filter.base_precision = None;
        instrument.lot_size_filter.quote_precision = None;

        let pair = Pair::new("BTC", "USDT");
        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "spot").unwrap();

        // Should still have other fields
        assert_eq!(trading_rules.min_order_size, Some(dec!(0.000011)));
        assert_eq!(trading_rules.tick_size, Some(dec!(0.1)));
        // Precision should be None if fields are missing
        assert_eq!(trading_rules.price_precision, None);
        assert_eq!(trading_rules.quantity_precision, None);
    }

    #[test]
    fn test_parse_trading_rules_handles_missing_price_scale() {
        let mut instrument = create_test_bybit_linear_instrument();
        instrument.price_scale = None;

        let pair = Pair::new("BTC", "USDT");
        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "linear").unwrap();

        // Should still have other fields
        assert_eq!(trading_rules.min_order_size, Some(dec!(0.001)));
        assert_eq!(trading_rules.tick_size, Some(dec!(0.10)));
        // Price precision should be None if price_scale is missing
        assert_eq!(trading_rules.price_precision, None);
    }

    #[test]
    fn test_parse_exchange_limits_returns_empty_rate_limits() {
        let exchange_limits = BybitPublicClient::parse_exchange_limits();

        assert_eq!(exchange_limits.rate_limits.len(), 0);
        assert_eq!(exchange_limits.max_position_size, None);
        assert_eq!(exchange_limits.max_open_orders, None);
        assert!(exchange_limits.other_limits.is_empty());
    }

    #[test]
    fn test_parse_exchange_info_handles_different_statuses() {
        let statuses = vec![
            ("Trading", SymbolStatus::Trading),
            ("PreLaunch", SymbolStatus::PreTrading),
            ("Settling", SymbolStatus::PostTrading),
            ("Delivering", SymbolStatus::EndOfDay),
            ("Closed", SymbolStatus::Halt),
            ("Unknown", SymbolStatus::Halt), // Should default to Halt
        ];

        for (status_string, expected_status) in statuses {
            let mut response = create_test_bybit_spot_response();
            response.result.list[0].status = SmolStr::new(status_string);

            let bybit_responses = vec![response];
            let requested_pairs = vec![Pair::new("BTC", "USDT")];

            let (symbol_info_map, _) =
                BybitPublicClient::parse_exchange_info(&bybit_responses, &requested_pairs).unwrap();

            let symbol_info = symbol_info_map.get(&Pair::new("BTC", "USDT")).unwrap();
            assert_eq!(
                symbol_info.status, expected_status,
                "Status '{}' should map to {:?}",
                status_string, expected_status
            );
        }
    }

    #[test]
    fn test_parse_trading_rules_handles_integer_precision() {
        let mut instrument = create_test_bybit_spot_instrument();
        instrument.lot_size_filter.base_precision = Some(dec!(1)); // Integer, no decimal
        instrument.lot_size_filter.quote_precision = Some(dec!(1));

        let pair = Pair::new("BTC", "USDT");
        let trading_rules =
            BybitPublicClient::parse_trading_rules(&instrument, &pair, "spot").unwrap();

        // Integer precision should result in 0 decimal places
        assert_eq!(trading_rules.quantity_precision, Some(0));
        assert_eq!(trading_rules.price_precision, Some(0));
    }
}
