//! k-paper public REST client: GET /exchange_info, map to ExchangeMarketData.

use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use tracing::info;
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

use crate::service::AssetDataClient;

/// k-paper GET /exchange_info response (minimal).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KPaperExchangeInfo {
    server_time: Option<u64>,
    rate_limits: Option<Vec<KPaperRateLimit>>,
    symbols: Vec<KPaperSymbol>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KPaperRateLimit {
    rate_limit_type: Option<String>,
    interval: Option<String>,
    limit: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KPaperSymbol {
    symbol: String,
    base_asset: String,
    quote_asset: String,
    status: Option<String>,
    trading_allowed: Option<bool>,
    min_order_size: Option<rust_decimal::Decimal>,
    max_order_size: Option<rust_decimal::Decimal>,
    step_size: Option<rust_decimal::Decimal>,
    min_price: Option<rust_decimal::Decimal>,
    max_price: Option<rust_decimal::Decimal>,
    tick_size: Option<rust_decimal::Decimal>,
    min_notional: Option<rust_decimal::Decimal>,
    price_precision: Option<u8>,
    quantity_precision: Option<u8>,
}

fn symbol_status(s: &str) -> SymbolStatus {
    match s.to_uppercase().as_str() {
        "TRADING" => SymbolStatus::Trading,
        "HALT" => SymbolStatus::Halt,
        "PRE_TRADING" | "PRETRADING" => SymbolStatus::PreTrading,
        "POST_TRADING" | "POSTTRADING" => SymbolStatus::PostTrading,
        "END_OF_DAY" => SymbolStatus::EndOfDay,
        "AUCTION_MATCH" => SymbolStatus::AuctionMatch,
        "BREAK" => SymbolStatus::Break,
        "PENDING_TRADING" => SymbolStatus::PendingTrading,
        _ => SymbolStatus::Trading,
    }
}

/// k-paper public REST client (GET /exchange_info).
#[derive(Debug)]
pub struct KPaperPublicClient {
    base_url: String,
    http_client: Client,
}

impl KPaperPublicClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http_client: Client::new(),
        }
    }

    async fn fetch_exchange_info(&self) -> Result<KPaperExchangeInfo, EngineError> {
        let url = format!("{}/exchange_info", self.base_url);
        info!("Fetching k-paper exchange info from {}", url);
        let res = self.http_client.get(&url).send().await.map_err(|e| {
            EngineError::CustomError(smol_str::format_smolstr!("HTTP error: {}", e))
        })?;
        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            return Err(EngineError::CustomError(smol_str::format_smolstr!(
                "GET /exchange_info failed: {} - {}",
                status,
                text
            )));
        }
        let info: KPaperExchangeInfo = res
            .json()
            .await
            .map_err(|e| EngineError::CustomError(smol_str::format_smolstr!("JSON: {}", e)))?;
        Ok(info)
    }

    fn build_market_data(
        &self,
        info: KPaperExchangeInfo,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError> {
        let pair_set: std::collections::HashSet<_> = pairs.iter().collect();
        let mut symbol_info = HashMap::new();
        let mut trading_rules = HashMap::new();

        for sym in &info.symbols {
            let pair = Pair::new(&sym.base_asset, &sym.quote_asset);
            if !pair_set.contains(&pair) {
                continue;
            }
            let status_str = sym.status.as_deref().unwrap_or("TRADING");
            let status = symbol_status(status_str);
            let trading_allowed = sym
                .trading_allowed
                .unwrap_or(status == SymbolStatus::Trading);
            let symbol_info_entry = SymbolInfo {
                pair: pair.clone(),
                status: status.clone(),
                base_asset: sym.base_asset.clone().into(),
                quote_asset: sym.quote_asset.clone().into(),
                exchange_symbol: sym.symbol.clone().into(),
                trading_allowed,
                is_active: trading_allowed,
            };
            symbol_info.insert(pair.clone(), symbol_info_entry);

            let rules = TradingRules {
                pair: pair.clone(),
                min_order_size: sym.min_order_size,
                max_order_size: sym.max_order_size,
                step_size: sym.step_size,
                min_price: sym.min_price,
                max_price: sym.max_price,
                tick_size: sym.tick_size,
                min_notional: sym.min_notional,
                max_notional: None,
                price_precision: sym.price_precision,
                quantity_precision: sym.quantity_precision,
            };
            trading_rules.insert(pair, rules);
        }

        let rate_limits = info
            .rate_limits
            .unwrap_or_default()
            .into_iter()
            .map(|r| RateLimit {
                rate_limit_type: match r.rate_limit_type.as_deref() {
                    Some("REQUEST_WEIGHT") => RateLimitType::RequestWeight,
                    Some("ORDERS") => RateLimitType::Orders,
                    _ => RateLimitType::RawRequests,
                },
                interval: r.interval.unwrap_or_else(|| "MINUTE".to_string()),
                limit: r.limit.unwrap_or(0),
            })
            .collect();

        let last_updated = info
            .server_time
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);

        Ok(ExchangeMarketData {
            symbol_info,
            trading_rules,
            current_prices: HashMap::new(),
            exchange_limits: ExchangeLimits {
                rate_limits,
                max_position_size: None,
                max_open_orders: None,
                other_limits: HashMap::new(),
            },
            last_updated,
        })
    }
}

#[async_trait]
impl AssetDataClient for KPaperPublicClient {
    async fn fetch_market_data(&self, pairs: &[Pair]) -> Result<ExchangeMarketData, EngineError> {
        let info = self.fetch_exchange_info().await?;
        info!(
            "Fetched k-paper exchange info: {} symbols",
            info.symbols.len()
        );
        self.build_market_data(info, pairs)
    }
}
