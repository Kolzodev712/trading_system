//! Asset Service - main service for fetching and caching asset data
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use trading_core::{
    error::EngineError,
    types::{
        asset::{ExchangeLimits, ExchangeMarketData, SymbolInfo, SymbolStatus, TradingRules},
        common::{Exchange, Pair},
    },
};

use crate::cache::AssetDataCache;
use crate::exchanges;
use crate::exchanges::binance::BinancePublicClient;

#[async_trait]
pub trait AssetDataClient: Send + Sync + std::fmt::Debug {
    /// Fetch exchange info (symbols, trading rules)
    async fn fetch_exchange_info(
        &self,
    ) -> Result<exchanges::binance::types::BinanceExchangeInfo, EngineError>;
}

/// Asset Service - orchestrates asset data fetching and caching
pub struct AssetService {
    cache: Arc<AssetDataCache>,
    exchange_clients: Arc<RwLock<HashMap<Exchange, Arc<dyn AssetDataClient>>>>,
}

impl AssetService {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(AssetDataCache::new(std::time::Duration::from_secs(300))), // 5 minutes TTL
            exchange_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an exchange client
    pub async fn register_exchange(
        &self,
        exchange: Exchange,
        client: Arc<dyn AssetDataClient>,
    ) -> Result<(), EngineError> {
        let mut clients = self.exchange_clients.write().await;
        clients.insert(exchange, client);
        Ok(())
    }

    /// Fetch comprehensive market data for pairs
    pub async fn fetch_market_data(
        &self,
        exchange: Exchange,
        pairs: &[Pair],
    ) -> Result<ExchangeMarketData, EngineError> {
        // Check cache first
        if let Some(cached) = self.cache.get(&exchange).await {
            if !self.cache.is_stale(&exchange).await {
                info!("Using cached market data for {:?}", exchange);
                return Ok(cached);
            }
            info!("Cache is stale for {:?}, fetching fresh data", exchange);
        }

        // Fetch fresh data
        // Get the client Arc and clone it so we can use it after dropping the lock
        let client = {
            let clients = self.exchange_clients.read().await;
            clients
                .get(&exchange)
                .ok_or_else(|| {
                    EngineError::CustomError(anyhow::anyhow!(
                        "Exchange {:?} not registered",
                        exchange
                    ))
                })?
                .clone()
        };

        let exchange_info = client.fetch_exchange_info().await?;

        // Parse exchange info into our types
        let (symbol_info, trading_rules) = Self::parse_exchange_info(&exchange_info, pairs)?;
        let exchange_limits = Self::parse_exchange_limits(&exchange_info);

        // Build market data
        let market_data = ExchangeMarketData {
            symbol_info,
            trading_rules,
            current_prices: HashMap::new(), // Will be populated by separate call if needed
            exchange_limits,
            last_updated: exchange_info.server_time,
        };

        // Update cache
        self.cache.update(exchange, market_data.clone()).await;

        info!(
            "Fetched market data: {} symbols, {} trading rules",
            market_data.symbol_info.len(),
            market_data.trading_rules.len()
        );

        Ok(market_data)
    }

    /// Get trading rules for a pair
    pub async fn get_trading_rules(&self, exchange: Exchange, pair: &Pair) -> Option<TradingRules> {
        if let Some(market_data) = self.cache.get(&exchange).await {
            market_data.trading_rules.get(pair).cloned()
        } else {
            None
        }
    }

    /// Get symbol info for a pair
    pub async fn get_symbol_info(&self, exchange: Exchange, pair: &Pair) -> Option<SymbolInfo> {
        if let Some(market_data) = self.cache.get(&exchange).await {
            market_data.symbol_info.get(pair).cloned()
        } else {
            None
        }
    }

    /// Parse Binance exchange info into our types
    fn parse_exchange_info(
        exchange_info: &exchanges::binance::types::BinanceExchangeInfo,
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

            // Create symbol info (clone pair for SymbolInfo)
            let symbol_info = SymbolInfo {
                pair: pair.clone(),
                status,
                base_asset: symbol.base_asset.clone(),
                quote_asset: symbol.quote_asset.clone(),
                exchange_symbol: symbol.symbol.clone(),
                trading_allowed,
                is_active,
            };

            // Parse trading rules from filters (clone pair for TradingRules)
            let trading_rules = Self::parse_trading_rules(symbol, &pair)?;

            // Insert both (pair is moved on first insert, so we need to clone)
            symbol_info_map.insert(pair.clone(), symbol_info);
            trading_rules_map.insert(pair, trading_rules);
        }

        Ok((symbol_info_map, trading_rules_map))
    }

    /// Parse trading rules from Binance symbol filters
    /// Returns TradingRules with Option fields - None if value not available from exchange
    /// The caller should fall back to config values for None fields
    fn parse_trading_rules(
        symbol: &exchanges::binance::types::BinanceSymbolInfo,
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
                exchanges::binance::types::BinanceFilter::LotSize {
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
                exchanges::binance::types::BinanceFilter::PriceFilter {
                    min_price: min,
                    max_price: max,
                    tick_size: tick,
                } => {
                    min_price = *min;
                    max_price = *max;
                    tick_size = *tick;
                }
                exchanges::binance::types::BinanceFilter::MinNotional {
                    min_notional: min_not,
                } => {
                    min_notional = Some(*min_not);
                }
                exchanges::binance::types::BinanceFilter::Other => {
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

    /// Parse exchange limits
    fn parse_exchange_limits(
        exchange_info: &exchanges::binance::types::BinanceExchangeInfo,
    ) -> ExchangeLimits {
        let rate_limits = BinancePublicClient::parse_rate_limits(&exchange_info.rate_limits);

        ExchangeLimits {
            rate_limits,
            max_position_size: None, // Not provided by Binance exchangeInfo
            max_open_orders: None,   // Not provided by Binance exchangeInfo
            other_limits: HashMap::new(),
        }
    }
}

impl Default for AssetService {
    fn default() -> Self {
        Self::new()
    }
}

// Implement AssetDataClient for BinancePublicClient
#[async_trait]
impl AssetDataClient for BinancePublicClient {
    async fn fetch_exchange_info(
        &self,
    ) -> Result<exchanges::binance::types::BinanceExchangeInfo, EngineError> {
        self.fetch_exchange_info().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::sync::Arc;
    use trading_core::types::asset::SymbolStatus;
    use trading_core::types::common::Pair;

    // Mock client for testing
    #[derive(Debug)]
    struct MockAssetDataClient {
        exchange_info: exchanges::binance::types::BinanceExchangeInfo,
    }

    #[async_trait]
    impl AssetDataClient for MockAssetDataClient {
        async fn fetch_exchange_info(
            &self,
        ) -> Result<exchanges::binance::types::BinanceExchangeInfo, EngineError> {
            Ok(self.exchange_info.clone())
        }
    }

    fn create_mock_exchange_info() -> exchanges::binance::types::BinanceExchangeInfo {
        use exchanges::binance::types::{BinanceFilter, BinanceRateLimit, BinanceSymbolInfo};
        use smol_str::SmolStr;

        exchanges::binance::types::BinanceExchangeInfo {
            server_time: 1234567890,
            rate_limits: vec![BinanceRateLimit {
                rate_limit_type: "REQUEST_WEIGHT".to_string(),
                interval: "MINUTE".to_string(),
                interval_num: 1,
                limit: 1200,
            }],
            symbols: vec![BinanceSymbolInfo {
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
            }],
        }
    }

    #[tokio::test]
    async fn test_asset_service_new() {
        let _service = AssetService::new();
        // Service should be created successfully
        assert!(true); // Just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_register_exchange() {
        let service = AssetService::new();
        let mock_client = Arc::new(MockAssetDataClient {
            exchange_info: create_mock_exchange_info(),
        });

        let result = service
            .register_exchange(Exchange::Binance, mock_client)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_market_data_unregistered_exchange() {
        let service = AssetService::new();
        let pairs = vec![Pair::new("BTC", "USDT")];

        let result = service.fetch_market_data(Exchange::Binance, &pairs).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not registered"));
    }

    #[tokio::test]
    async fn test_fetch_market_data() {
        let service = AssetService::new();
        let mock_client = Arc::new(MockAssetDataClient {
            exchange_info: create_mock_exchange_info(),
        });

        service
            .register_exchange(Exchange::Binance, mock_client)
            .await
            .unwrap();

        let pairs = vec![Pair::new("BTC", "USDT")];
        let result = service.fetch_market_data(Exchange::Binance, &pairs).await;

        assert!(result.is_ok());
        let market_data = result.unwrap();
        assert_eq!(market_data.symbol_info.len(), 1);
        assert_eq!(market_data.trading_rules.len(), 1);
        assert!(market_data
            .symbol_info
            .contains_key(&Pair::new("BTC", "USDT")));
    }

    #[tokio::test]
    async fn test_get_trading_rules() {
        let service = AssetService::new();
        let mock_client = Arc::new(MockAssetDataClient {
            exchange_info: create_mock_exchange_info(),
        });

        service
            .register_exchange(Exchange::Binance, mock_client)
            .await
            .unwrap();

        let pairs = vec![Pair::new("BTC", "USDT")];
        service
            .fetch_market_data(Exchange::Binance, &pairs)
            .await
            .unwrap();

        let rules = service
            .get_trading_rules(Exchange::Binance, &Pair::new("BTC", "USDT"))
            .await;

        assert!(rules.is_some());
        let rules = rules.unwrap();
        assert_eq!(rules.pair, Pair::new("BTC", "USDT"));
        assert!(rules.step_size.is_some());
        assert!(rules.tick_size.is_some());
    }

    #[tokio::test]
    async fn test_get_symbol_info() {
        let service = AssetService::new();
        let mock_client = Arc::new(MockAssetDataClient {
            exchange_info: create_mock_exchange_info(),
        });

        service
            .register_exchange(Exchange::Binance, mock_client)
            .await
            .unwrap();

        let pairs = vec![Pair::new("BTC", "USDT")];
        service
            .fetch_market_data(Exchange::Binance, &pairs)
            .await
            .unwrap();

        let symbol_info = service
            .get_symbol_info(Exchange::Binance, &Pair::new("BTC", "USDT"))
            .await;

        assert!(symbol_info.is_some());
        let symbol_info = symbol_info.unwrap();
        assert_eq!(symbol_info.pair, Pair::new("BTC", "USDT"));
        assert_eq!(symbol_info.status, SymbolStatus::Trading);
        assert!(symbol_info.trading_allowed);
    }

    #[tokio::test]
    async fn test_cache_usage() {
        let service = AssetService::new();
        let mock_client = Arc::new(MockAssetDataClient {
            exchange_info: create_mock_exchange_info(),
        });

        service
            .register_exchange(Exchange::Binance, mock_client)
            .await
            .unwrap();

        let pairs = vec![Pair::new("BTC", "USDT")];

        // First fetch - should hit the client
        let result1 = service.fetch_market_data(Exchange::Binance, &pairs).await;
        assert!(result1.is_ok());

        // Second fetch - should use cache (if not stale)
        let result2 = service.fetch_market_data(Exchange::Binance, &pairs).await;
        assert!(result2.is_ok());
    }

    #[test]
    fn test_parse_trading_rules() {
        use exchanges::binance::types::{BinanceFilter, BinanceSymbolInfo};
        use smol_str::SmolStr;

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
        let rules = AssetService::parse_trading_rules(&symbol, &pair).unwrap();

        assert_eq!(rules.pair, pair);
        assert_eq!(rules.min_order_size, Some(dec!(0.00001)));
        assert_eq!(rules.max_order_size, Some(dec!(9000)));
        assert_eq!(rules.step_size, Some(dec!(0.00001)));
        assert_eq!(rules.tick_size, Some(dec!(0.01)));
        assert_eq!(rules.min_notional, Some(dec!(10)));
        assert_eq!(rules.price_precision, Some(8));
        assert_eq!(rules.quantity_precision, Some(8));
    }
}
