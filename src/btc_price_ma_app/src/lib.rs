//! Candidate #2 composition: adapter + factory injected into the generic runner.

mod adapter;
mod factory;

pub use adapter::{
    BtcPriceMaLongFlatV1, BtcPriceMaLongFlatV1Config, EntryFreshness, Readiness, RecoveryState,
    RuntimeTarget, RESEARCH_DISPOSITION_REJECTED_ALPHA,
};
pub use factory::BtcPriceMaFactory;
