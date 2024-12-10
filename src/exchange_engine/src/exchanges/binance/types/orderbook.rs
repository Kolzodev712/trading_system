use core_logic::{
    data::public_data::Orderbook,
    utils::{deserialize_smol_str, serialize_smol_str},
};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#order-book>
pub struct BinanceOrderbookSnapshotRequest {
    #[serde(
        serialize_with = "serialize_smol_str",
        deserialize_with = "deserialize_smol_str"
    )]
    pub symbol: SmolStr,
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// <https://binance-docs.github.io/apidocs/spot/en/#order-book>
pub struct BinanceOrderbookSnapshotResponse {
    pub last_update_id: u64,
    pub bids: Vec<[Decimal; 2]>,
    pub asks: Vec<[Decimal; 2]>,
}

impl From<BinanceOrderbookSnapshotResponse> for Orderbook {
    fn from(snapshot: BinanceOrderbookSnapshotResponse) -> Self {
        Orderbook {
            bids: snapshot
                .bids
                .into_iter()
                .flat_map(|bid| vec![bid[0], bid[1]])
                .collect(),
            asks: snapshot
                .asks
                .into_iter()
                .flat_map(|ask| vec![ask[0], ask[1]])
                .collect(),
        }
    }
}
