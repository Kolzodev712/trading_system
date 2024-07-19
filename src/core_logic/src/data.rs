// General level data abstractions:

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrderBook {
    pub bids: Vec<Decimal>,
    pub asks: Vec<Decimal>,
}
