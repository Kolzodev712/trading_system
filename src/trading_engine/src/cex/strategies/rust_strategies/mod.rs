//! Individual trading strategy implementations.

mod low_high;
mod prop_hold;

pub use low_high::BuyLowSellHigh;
pub use prop_hold::PropHold;
