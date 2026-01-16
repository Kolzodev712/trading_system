//! Binance public REST client for fetching exchange information (unsigned calls)
use super::types::BinanceExchangeInfo;
use crate::service::AssetDataClient;
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use tracing::{error, info};
use trading_core::{
    error::EngineError,
    types::{
        asset::{
            ExchangeLimits, ExchangeMarketData, RateLimit, RateLimitType, SymbolInfo, SymbolStatus,
            TradingRules,
        },
        common::Pair,
    },
};

/// Binance public REST client (no authentication required)
#[derive(Debug)]
pub struct BinancePublicClient {
    base_url: String,
    http_client: Client,
}

impl BinancePublicClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http_client: Client::new(),
        }
    }

    /// Fetch exchange info (public endpoint, no auth)
    async fn fetch_exchange_info(&self) -> Result<BinanceExchangeInfo, EngineError> {
        let url = format!("{}/api/v3/exchangeInfo", self.base_url);
        info!("Fetching exchange info from {}", url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("HTTP error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Exchange info request failed: {} - {}", status, text);
            return Err(EngineError::CustomError(anyhow::anyhow!(
                "Exchange info request failed: {} - {}",
                status,
                text
            )));
        }

        let exchange_info: BinanceExchangeInfo = response
            .json()
            .await
            .map_err(|e| EngineError::CustomError(anyhow::anyhow!("JSON parse error: {}", e)))?;

        info!(
            "Fetched exchange info: {} symbols, {} rate limits",
            exchange_info.symbols.len(),
            exchange_info.rate_limits.len()
        );

        Ok(exchange_info)
    }

    /// Parse Binance exchange info into common types
    fn parse_exchange_info(
        exchange_info: &BinanceExchangeInfo,
        pairs: &[Pair],
    ) -> Result<(HashMap<Pair, SymbolInfo>, HashMap<Pair, TradingRules>), EngineError> {
        let mut symbol_info_map = HashMap::new();
        let mut trading_rules_map = HashMap::new();

        // Create a set of pairs we're interested in (for filtering)
        let pair_set: std::collections::HashSet<_> = pairs.iter().collect();

        for symbol in &exchange_info.symbols {
            // Convert symbol to Pair
            let pair = Pair::new(symbol.base_asset.as_str(), symbol.quote_asset.as_str());

            // Only process pairs we're interested in
            if !pair_set.contains(&pair) {
                continue;
            }

            // Parse symbol status
            let status = match symbol.status.as_ref() {
                "PRE_TRADING" => SymbolStatus::PreTrading,
                "TRADING" => SymbolStatus::Trading,
                "POST_TRADING" => SymbolStatus::PostTrading,
                "END_OF_DAY" => SymbolStatus::EndOfDay,
                "HALT" => SymbolStatus::Halt,
                "AUCTION_MATCH" => SymbolStatus::AuctionMatch,
                "BREAK" => SymbolStatus::Break,
                "PENDING_TRADING" => SymbolStatus::PendingTrading,
                _ => SymbolStatus::Halt, // Default to halt for unknown status
            };

            let trading_allowed = status == SymbolStatus::Trading;
            let is_active = trading_allowed && symbol.is_spot_trading_allowed;

            // Create symbol info
            let symbol_info = SymbolInfo {
                pair: pair.clone(),
                status,
                base_asset: symbol.base_asset.clone(),
                quote_asset: symbol.quote_asset.clone(),
                exchange_symbol: symbol.symbol.clone(),
                trading_allowed,
                is_active,
            };

            // Parse trading rules from filters
            let trading_rules = Self::parse_trading_rules(symbol, &pair)?;

            // Insert both
            symbol_info_map.insert(pair.clone(), symbol_info);
            trading_rules_map.insert(pair, trading_rules);
        }

        Ok((symbol_info_map, trading_rules_map))
    }

    /// Parse trading rules from Binance symbol filters
    fn parse_trading_rules(
        symbol: &crate::exchanges::binance::types::BinanceSymbolInfo,
        pair: &Pair,
    ) -> Result<TradingRules, EngineError> {
        use rust_decimal::Decimal;

        let mut min_order_size = None;
        let mut max_order_size = None;
        let mut step_size = None;
        let mut min_price = None;
        let mut max_price = None;
        let mut tick_size = None;
        let mut min_notional = None;

        // Parse filters - only set values if they exist in the exchange response
        for filter in &symbol.filters {
            match filter {
                crate::exchanges::binance::types::BinanceFilter::LotSize {
                    min_qty,
                    max_qty,
                    step_size: step,
                } => {
                    min_order_size = Some(*min_qty);
                    if *max_qty != Decimal::ZERO {
                        max_order_size = Some(*max_qty);
                    }
                    step_size = Some(*step);
                }
                crate::exchanges::binance::types::BinanceFilter::PriceFilter {
                    min_price: min,
                    max_price: max,
                    tick_size: tick,
                } => {
                    min_price = *min;
                    max_price = *max;
                    tick_size = *tick;
                }
                crate::exchanges::binance::types::BinanceFilter::MinNotional {
                    min_notional: min_not,
                } => {
                    min_notional = Some(*min_not);
                }
                crate::exchanges::binance::types::BinanceFilter::Other => {
                    // Other filters not used for trading rules - ignore
                }
            }
        }

        Ok(TradingRules {
            pair: pair.clone(),
            min_order_size,
            max_order_size,
            step_size,
            min_price,
            max_price,
            tick_size,
            min_notional,
            max_notional: None, // Not provided by Binance
            price_precision: Some(symbol.quote_precision),
            quantity_precision: Some(symbol.base_asset_precision),
        })
    }

    /// Parse Binance exchange limits
    fn parse_exchange_limits(exchange_info: &BinanceExchangeInfo) -> ExchangeLimits {
        let rate_limits = exchange_info
            .rate_limits
            .iter()
            .map(|limit| RateLimit {
                rate_limit_type: match limit.rate_limit_type.as_str() {
                    "REQUEST_WEIGHT" => RateLimitType::RequestWeight,
                    "ORDERS" => RateLimitType::Orders,
                    "RAW_REQUESTS" => RateLimitType::RawRequests,
                    _ => RateLimitType::RawRequests, // Default fallback
                },
                interval: limit.interval.clone(),
                limit: limit.limit,
            })
            .collect();

        ExchangeLimits {
            rate_limits,
            max_position_size: None, // Not provided by Binance exchangeInfo
            max_open_orders: None,   // Not provided by Binance exchangeInfo
            other_limits: HashMap::new(),
        }
    }
}

// Implement AssetDataClient for BinancePublicClient
#[async_trait]
impl AssetDataClient for BinancePublicClient {
    async fn fetch_market_data(&self, pairs: &[Pair]) -> Result<ExchangeMarketData, EngineError> {
        let exchange_info = self.fetch_exchange_info().await?;

        // Parse exchange info into common types
        let (symbol_info_map, trading_rules_map) =
            Self::parse_exchange_info(&exchange_info, pairs)?;
        let exchange_limits = Self::parse_exchange_limits(&exchange_info);

        // Build market data
        let market_data = ExchangeMarketData {
            symbol_info: symbol_info_map,
            trading_rules: trading_rules_map,
            current_prices: HashMap::new(), // Will be populated by separate call if needed
            exchange_limits,
            last_updated: exchange_info.server_time,
        };

        Ok(market_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchanges::binance::types::{
        BinanceExchangeInfo, BinanceFilter, BinanceRateLimit, BinanceSymbolInfo,
    };
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;
    use trading_core::types::asset::{RateLimitType, SymbolStatus};

    fn create_test_binance_exchange_info() -> BinanceExchangeInfo {
        BinanceExchangeInfo {
            server_time: 1734819696263,
            rate_limits: vec![
                BinanceRateLimit {
                    rate_limit_type: "REQUEST_WEIGHT".to_string(),
                    interval: "MINUTE".to_string(),
                    interval_num: 1,
                    limit: 1200,
                },
                BinanceRateLimit {
                    rate_limit_type: "ORDERS".to_string(),
                    interval: "SECOND".to_string(),
                    interval_num: 1,
                    limit: 10,
                },
            ],
            symbols: vec![
                BinanceSymbolInfo {
                    symbol: SmolStr::new("BTCUSDT"),
                    status: SmolStr::new("TRADING"),
                    base_asset: SmolStr::new("BTC"),
                    base_asset_precision: 8,
                    quote_asset: SmolStr::new("USDT"),
                    quote_precision: 8,
                    is_spot_trading_allowed: true,
                    filters: vec![
                        BinanceFilter::LotSize {
                            min_qty: dec!(0.00001),
                            max_qty: dec!(9000),
                            step_size: dec!(0.00001),
                        },
                        BinanceFilter::PriceFilter {
                            min_price: Some(dec!(0.01)),
                            max_price: Some(dec!(1000000)),
                            tick_size: Some(dec!(0.01)),
                        },
                        BinanceFilter::MinNotional {
                            min_notional: dec!(10),
                        },
                    ],
                },
                BinanceSymbolInfo {
                    symbol: SmolStr::new("ETHUSDT"),
                    status: SmolStr::new("TRADING"),
                    base_asset: SmolStr::new("ETH"),
                    base_asset_precision: 8,
                    quote_asset: SmolStr::new("USDT"),
                    quote_precision: 8,
                    is_spot_trading_allowed: true,
                    filters: vec![
                        BinanceFilter::LotSize {
                            min_qty: dec!(0.001),
                            max_qty: dec!(100000),
                            step_size: dec!(0.001),
                        },
                        BinanceFilter::PriceFilter {
                            min_price: Some(dec!(0.01)),
                            max_price: Some(dec!(100000)),
                            tick_size: Some(dec!(0.01)),
                        },
                        BinanceFilter::MinNotional {
                            min_notional: dec!(5),
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn test_parse_exchange_info_filters_by_pairs() {
        let exchange_info = create_test_binance_exchange_info();
        let requested_pairs = vec![Pair::new("BTC", "USDT")];

        let (symbol_info_map, trading_rules_map) =
            BinancePublicClient::parse_exchange_info(&exchange_info, &requested_pairs).unwrap();

        // Should only have BTCUSDT, not ETHUSDT
        assert_eq!(symbol_info_map.len(), 1);
        assert_eq!(trading_rules_map.len(), 1);
        assert!(symbol_info_map.contains_key(&Pair::new("BTC", "USDT")));
        assert!(!symbol_info_map.contains_key(&Pair::new("ETH", "USDT")));
    }

    #[test]
    fn test_parse_exchange_info_handles_multiple_pairs() {
        let exchange_info = create_test_binance_exchange_info();
        let requested_pairs = vec![Pair::new("BTC", "USDT"), Pair::new("ETH", "USDT")];

        let (symbol_info_map, trading_rules_map) =
            BinancePublicClient::parse_exchange_info(&exchange_info, &requested_pairs).unwrap();

        assert_eq!(symbol_info_map.len(), 2);
        assert_eq!(trading_rules_map.len(), 2);
        assert!(symbol_info_map.contains_key(&Pair::new("BTC", "USDT")));
        assert!(symbol_info_map.contains_key(&Pair::new("ETH", "USDT")));
    }

    #[test]
    fn test_parse_exchange_info_symbol_status_mapping() {
        let mut exchange_info = create_test_binance_exchange_info();
        exchange_info.symbols[0].status = SmolStr::new("HALT");

        let requested_pairs = vec![Pair::new("BTC", "USDT")];
        let (symbol_info_map, _) =
            BinancePublicClient::parse_exchange_info(&exchange_info, &requested_pairs).unwrap();

        let symbol_info = symbol_info_map.get(&Pair::new("BTC", "USDT")).unwrap();
        assert_eq!(symbol_info.status, SymbolStatus::Halt);
        assert!(!symbol_info.trading_allowed);
    }

    #[test]
    fn test_parse_trading_rules_extracts_all_filters() {
        let symbol = BinanceSymbolInfo {
            symbol: SmolStr::new("BTCUSDT"),
            status: SmolStr::new("TRADING"),
            base_asset: SmolStr::new("BTC"),
            base_asset_precision: 8,
            quote_asset: SmolStr::new("USDT"),
            quote_precision: 8,
            is_spot_trading_allowed: true,
            filters: vec![
                BinanceFilter::LotSize {
                    min_qty: dec!(0.00001),
                    max_qty: dec!(9000),
                    step_size: dec!(0.00001),
                },
                BinanceFilter::PriceFilter {
                    min_price: Some(dec!(0.01)),
                    max_price: Some(dec!(1000000)),
                    tick_size: Some(dec!(0.01)),
                },
                BinanceFilter::MinNotional {
                    min_notional: dec!(10),
                },
            ],
        };

        let pair = Pair::new("BTC", "USDT");
        let trading_rules = BinancePublicClient::parse_trading_rules(&symbol, &pair).unwrap();

        assert_eq!(trading_rules.pair, pair);
        assert_eq!(trading_rules.min_order_size, Some(dec!(0.00001)));
        assert_eq!(trading_rules.max_order_size, Some(dec!(9000)));
        assert_eq!(trading_rules.step_size, Some(dec!(0.00001)));
        assert_eq!(trading_rules.min_price, Some(dec!(0.01)));
        assert_eq!(trading_rules.max_price, Some(dec!(1000000)));
        assert_eq!(trading_rules.tick_size, Some(dec!(0.01)));
        assert_eq!(trading_rules.min_notional, Some(dec!(10)));
        assert_eq!(trading_rules.price_precision, Some(8));
        assert_eq!(trading_rules.quantity_precision, Some(8));
    }

    #[test]
    fn test_parse_trading_rules_handles_zero_max_qty() {
        let symbol = BinanceSymbolInfo {
            symbol: SmolStr::new("BTCUSDT"),
            status: SmolStr::new("TRADING"),
            base_asset: SmolStr::new("BTC"),
            base_asset_precision: 8,
            quote_asset: SmolStr::new("USDT"),
            quote_precision: 8,
            is_spot_trading_allowed: true,
            filters: vec![BinanceFilter::LotSize {
                min_qty: dec!(0.00001),
                max_qty: dec!(0), // Zero means no limit
                step_size: dec!(0.00001),
            }],
        };

        let pair = Pair::new("BTC", "USDT");
        let trading_rules = BinancePublicClient::parse_trading_rules(&symbol, &pair).unwrap();

        assert_eq!(trading_rules.max_order_size, None); // Zero max_qty should result in None
    }

    #[test]
    fn test_parse_trading_rules_handles_missing_filters() {
        let symbol = BinanceSymbolInfo {
            symbol: SmolStr::new("BTCUSDT"),
            status: SmolStr::new("TRADING"),
            base_asset: SmolStr::new("BTC"),
            base_asset_precision: 8,
            quote_asset: SmolStr::new("USDT"),
            quote_precision: 8,
            is_spot_trading_allowed: true,
            filters: vec![], // No filters
        };

        let pair = Pair::new("BTC", "USDT");
        let trading_rules = BinancePublicClient::parse_trading_rules(&symbol, &pair).unwrap();

        assert_eq!(trading_rules.min_order_size, None);
        assert_eq!(trading_rules.max_order_size, None);
        assert_eq!(trading_rules.step_size, None);
        assert_eq!(trading_rules.tick_size, None);
        assert_eq!(trading_rules.min_notional, None);
        // Precision should still be set from symbol fields
        assert_eq!(trading_rules.price_precision, Some(8));
        assert_eq!(trading_rules.quantity_precision, Some(8));
    }

    #[test]
    fn test_parse_exchange_limits_converts_rate_limits() {
        let exchange_info = create_test_binance_exchange_info();
        let exchange_limits = BinancePublicClient::parse_exchange_limits(&exchange_info);

        assert_eq!(exchange_limits.rate_limits.len(), 2);

        let request_weight_limit = exchange_limits
            .rate_limits
            .iter()
            .find(|limit| limit.rate_limit_type == RateLimitType::RequestWeight)
            .unwrap();
        assert_eq!(request_weight_limit.limit, 1200);
        assert_eq!(request_weight_limit.interval, "MINUTE");

        let orders_limit = exchange_limits
            .rate_limits
            .iter()
            .find(|limit| limit.rate_limit_type == RateLimitType::Orders)
            .unwrap();
        assert_eq!(orders_limit.limit, 10);
        assert_eq!(orders_limit.interval, "SECOND");
    }

    #[test]
    fn test_parse_exchange_limits_handles_unknown_rate_limit_type() {
        let mut exchange_info = create_test_binance_exchange_info();
        exchange_info.rate_limits.push(BinanceRateLimit {
            rate_limit_type: "UNKNOWN_TYPE".to_string(),
            interval: "HOUR".to_string(),
            interval_num: 1,
            limit: 100,
        });

        let exchange_limits = BinancePublicClient::parse_exchange_limits(&exchange_info);

        // Unknown type should default to RawRequests
        let unknown_limit = exchange_limits
            .rate_limits
            .iter()
            .find(|limit| limit.rate_limit_type == RateLimitType::RawRequests && limit.limit == 100)
            .unwrap();
        assert_eq!(unknown_limit.interval, "HOUR");
    }

    #[test]
    fn test_parse_exchange_info_handles_inactive_symbols() {
        let mut exchange_info = create_test_binance_exchange_info();
        exchange_info.symbols[0].is_spot_trading_allowed = false;

        let requested_pairs = vec![Pair::new("BTC", "USDT")];
        let (symbol_info_map, _) =
            BinancePublicClient::parse_exchange_info(&exchange_info, &requested_pairs).unwrap();

        let symbol_info = symbol_info_map.get(&Pair::new("BTC", "USDT")).unwrap();
        assert!(!symbol_info.is_active);
    }
}
