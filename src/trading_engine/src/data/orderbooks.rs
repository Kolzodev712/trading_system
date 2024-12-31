use core_logic::data::public_data::Orderbook;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use tracing::warn;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderbookProcessor {
    data: Vec<Orderbook>,
    processed_data: Vec<ProcessedOrderbook>,
}

impl OrderbookProcessor {
    pub fn new(data: Vec<Orderbook>) -> Self {
        let mut processor = Self {
            data,
            processed_data: Vec::new(),
        };
        processor.sort_orderbook();
        processor.process_data();
        processor
    }

    pub fn process_data(&mut self) {
        self.processed_data = self
            .data
            .iter()
            .map(|orderbook| ProcessedOrderbook::from_orderbook(orderbook))
            .collect();
    }

    pub fn sort_orderbook(&mut self) {
        for (index, orderbook) in self.data.iter_mut().enumerate() {
            if orderbook.bids.is_empty() || orderbook.asks.is_empty() {
                warn!(
                    "Warning: Orderbook at index {} has empty bids or asks",
                    index
                );
                continue;
            }

            orderbook
                .bids
                .sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            orderbook
                .asks
                .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessedOrderbook {
    midprice: Decimal,
    best_bid: Decimal,
    best_ask: Decimal,
    spread: Decimal,
}

impl ProcessedOrderbook {
    fn new() -> Self {
        Self {
            midprice: Decimal::ZERO,
            best_bid: Decimal::ZERO,
            best_ask: Decimal::ZERO,
            spread: Decimal::ZERO,
        }
    }

    pub fn from_orderbook(orderbook: &Orderbook) -> Self {
        if orderbook.bids.is_empty() || orderbook.asks.is_empty() {
            return Self::new();
        }

        let best_bid = orderbook.bids[0];
        let best_ask = orderbook.asks[0];
        let midprice = (best_bid + best_ask) / Decimal::from(2);
        let spread = best_ask - best_bid;

        Self {
            midprice,
            best_bid,
            best_ask,
            spread,
        }
    }

    pub fn validate(&self) -> bool {
        self.best_bid <= self.best_ask && self.spread >= Decimal::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::*;
    use serde_json;

    // Raw JSON for testing
    const RAW_ORDERBOOK_JSON: &str = r#"
    {
        "bids": [93717.78000000, 93714.95000000, 93713.99000000],
        "asks": [93717.79000000, 93719.49000000, 93720.00000000]
    }
    "#;

    #[test]
    fn test_serialization_and_deserialization() {
        let orderbook: Orderbook = serde_json::from_str(RAW_ORDERBOOK_JSON).unwrap();

        assert_eq!(orderbook.bids[0], Decimal::from_str("93717.78").unwrap());
        assert_eq!(orderbook.asks[0], Decimal::from_str("93717.79").unwrap());

        let serialized = serde_json::to_string(&orderbook).unwrap();
        let deserialized_back: Orderbook = serde_json::from_str(&serialized).unwrap();
        assert_eq!(orderbook, deserialized_back);
    }

    #[test]
    fn test_processed_orderbook_from_orderbook() {
        let orderbook: Orderbook = serde_json::from_str(RAW_ORDERBOOK_JSON).unwrap();

        let processed = ProcessedOrderbook::from_orderbook(&orderbook);

        assert_eq!(processed.best_bid, Decimal::from_str("93717.78").unwrap());
        assert_eq!(processed.best_ask, Decimal::from_str("93717.79").unwrap());
        assert_eq!(processed.midprice, Decimal::from_str("93717.785").unwrap()); // (93717.780 + 93717.790) / 2
        assert_eq!(processed.spread, Decimal::from_str("0.01").unwrap()); // 93717.790 - 93717.780
        assert!(processed.validate());
    }

    #[test]
    fn test_orderbook_processor_workflow() {
        let orderbook: Orderbook = serde_json::from_str(RAW_ORDERBOOK_JSON).unwrap();

        let processor = OrderbookProcessor::new(vec![orderbook.clone()]);

        assert_eq!(processor.data.len(), 1);
        assert_eq!(
            processor.data[0].bids[0],
            Decimal::from_str("93717.78").unwrap()
        );
        assert_eq!(
            processor.data[0].asks[0],
            Decimal::from_str("93717.79").unwrap()
        );

        let processed = &processor.processed_data[0];
        assert_eq!(processed.best_bid, Decimal::from_str("93717.78").unwrap());
        assert_eq!(processed.best_ask, Decimal::from_str("93717.79").unwrap());
        assert_eq!(processed.midprice, Decimal::from_str("93717.785").unwrap());
        assert_eq!(processed.spread, Decimal::from_str("0.01").unwrap());
    }
}
