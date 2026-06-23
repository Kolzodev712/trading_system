//! Shared types and helpers for Sui gRPC write-path calls.

use smol_str::SmolStr;
use sui_rpc::proto::sui::rpc::v2::{changed_object, ChangedObject, ExecutionStatus};
use sui_sdk_types::{Address, Digest};
use sui_transaction_builder::ObjectInput;
use trading_core::venue::dex::types::TxStatus;
use trading_core::venue::dex::DexError;

pub const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

#[derive(Debug, Clone)]
pub struct CoinRef {
    pub object_id: Address,
    pub version: u64,
    pub digest: Digest,
    pub balance: u64,
}

#[derive(Debug, Clone)]
pub struct SharedObjectRef {
    pub object_id: Address,
    pub initial_shared_version: u64,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecuteTxResult {
    pub digest: SmolStr,
    pub status: TxStatus,
    pub changed_objects: Vec<ChangedObject>,
}

pub(crate) fn find_created_in_changes(
    changes: &[ChangedObject],
    type_contains: &str,
) -> Option<Address> {
    for change in changes {
        if change.id_operation != Some(changed_object::IdOperation::Created as i32) {
            continue;
        }
        let object_type = change.object_type_opt()?;
        if object_type.contains(type_contains) {
            return change.object_id_opt()?.parse().ok();
        }
    }
    None
}

pub fn pick_gas_coin(coins: &[CoinRef]) -> Result<&CoinRef, DexError> {
    coins
        .iter()
        .max_by_key(|c| c.balance)
        .ok_or_else(|| DexError::Transaction("no SUI coins available for gas".into()))
}

/// When depositing SUI, reserve the **smallest** viable gas coin so larger coins stay available
/// as deposit inputs (avoid `pick_gas_coin` taking the largest coin as gas).
pub fn pick_sui_deposit_coins(
    coins: &[CoinRef],
    deposit_needed: u64,
    gas_budget: u64,
) -> Result<(&CoinRef, &CoinRef), DexError> {
    if coins.is_empty() {
        return Err(DexError::Transaction("no SUI coins available".into()));
    }

    let funding = coins
        .iter()
        .filter(|c| c.balance >= deposit_needed)
        .max_by_key(|c| c.balance)
        .ok_or_else(|| {
            let max_single = coins.iter().map(|c| c.balance).max().unwrap_or(0);
            let total: u64 = coins.iter().map(|c| c.balance).sum();
            DexError::Transaction(
                format!(
                    "insufficient SUI for deposit: need {deposit_needed} mist in one coin object, \
                     largest coin has {max_single} mist (total across {} objects: {total} mist)",
                    coins.len()
                )
                .into(),
            )
        })?;

    if let Some(gas) = coins
        .iter()
        .filter(|c| c.object_id != funding.object_id && c.balance >= gas_budget)
        .min_by_key(|c| c.balance)
    {
        return Ok((gas, funding));
    }

    Err(DexError::Transaction(
        format!(
            "no separate SUI coin for gas (need {gas_budget} mist); \
             free a small coin object or reduce deposit below largest coin balance"
        )
        .into(),
    ))
}

pub fn coin_to_object_input(coin: &CoinRef) -> ObjectInput {
    ObjectInput::owned(coin.object_id, coin.version, coin.digest)
}

pub(crate) fn map_execution_status(status: &ExecutionStatus) -> TxStatus {
    match status.success_opt() {
        Some(true) => TxStatus::Confirmed,
        Some(false) => TxStatus::Failed,
        None => TxStatus::Pending,
    }
}

pub(crate) fn rpc_err(msg: impl Into<SmolStr>) -> DexError {
    DexError::Rpc(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use changed_object::IdOperation;

    #[test]
    fn find_created_object_in_changed_objects() {
        let changes = vec![ChangedObject::default()
            .with_object_id("0x71483f6105efc83797dd937d9d15be8f8e8eab124bceeed41484c440368595fc")
            .with_id_operation(IdOperation::Created)
            .with_object_type("0xfb28::balance_manager::BalanceManager")];
        let id = find_created_in_changes(&changes, "balance_manager::BalanceManager").unwrap();
        assert_eq!(
            id.to_string(),
            "0x71483f6105efc83797dd937d9d15be8f8e8eab124bceeed41484c440368595fc"
        );
    }

    #[test]
    fn map_status_from_execution_status() {
        assert_eq!(
            map_execution_status(&ExecutionStatus::default().with_success(true)),
            TxStatus::Confirmed
        );
        assert_eq!(
            map_execution_status(&ExecutionStatus::default().with_success(false)),
            TxStatus::Failed
        );
    }

    fn coin(byte: u8, balance: u64) -> CoinRef {
        use sui_sdk_types::{Address, Digest};
        let mut id = [0u8; 32];
        id[31] = byte;
        CoinRef {
            object_id: Address::new(id),
            version: 1,
            digest: Digest::new([byte; 32]),
            balance,
        }
    }

    #[test]
    fn pick_sui_deposit_uses_smallest_gas_coin() {
        let coins = vec![
            coin(1, 1_000_000_000), // 1 SUI — funding
            coin(2, 984_764_000),
            coin(3, 374_305_000), // gas
        ];
        let (gas, fund) = pick_sui_deposit_coins(&coins, 1_000_000_000, 250_000_000).unwrap();
        assert_eq!(gas.object_id, coin(3, 0).object_id);
        assert_eq!(fund.object_id, coin(1, 0).object_id);
    }

    #[test]
    fn pick_sui_deposit_rejects_when_no_single_coin_large_enough() {
        let coins = vec![coin(1, 1_000_000_000), coin(2, 984_764_000)];
        assert!(pick_sui_deposit_coins(&coins, 1_500_000_000, 250_000_000).is_err());
    }
}
