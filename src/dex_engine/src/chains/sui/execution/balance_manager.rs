//! DeepBook `BalanceManager` setup PTBs (create → register → share).

use sui_sdk_types::{Address, Identifier, TypeTag};
use sui_transaction_builder::{Function, ObjectInput, TransactionBuilder};
use trading_core::venue::dex::DexError;

use super::ptb::{id_err, parse_type_tag};
use super::rpc_types::SharedObjectRef;

/// Append create → register → share for a new DeepBook [`BalanceManager`](https://docs.sui.io/onchain-finance/deepbookv3/contract-information/balance-manager).
///
/// Matches the bundled PTB used by the DeepBook dashboard / sandbox
/// (`balance_manager::new`, `register_balance_manager`, `public_share_object`).
pub fn append_create_register_and_share(
    tx: &mut TransactionBuilder,
    package_id: Address,
    registry: &SharedObjectRef,
) -> Result<(), DexError> {
    let manager = tx.move_call(
        Function::new(
            package_id,
            Identifier::new("balance_manager").map_err(id_err)?,
            Identifier::new("new").map_err(id_err)?,
        ),
        vec![],
    );

    let registry_input = tx.object(ObjectInput::shared(
        registry.object_id,
        registry.initial_shared_version,
        registry.mutable,
    ));
    tx.move_call(
        Function::new(
            package_id,
            Identifier::new("balance_manager").map_err(id_err)?,
            Identifier::new("register_balance_manager").map_err(id_err)?,
        ),
        vec![manager, registry_input],
    );

    let bm_type = balance_manager_type_tag(package_id)?;
    tx.move_call(
        Function::new(
            Address::TWO,
            Identifier::new("transfer").map_err(id_err)?,
            Identifier::new("public_share_object").map_err(id_err)?,
        )
        .with_type_args(vec![bm_type]),
        vec![manager],
    );

    Ok(())
}

fn balance_manager_type_tag(package_id: Address) -> Result<TypeTag, DexError> {
    parse_type_tag(&format!("{package_id}::balance_manager::BalanceManager"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_manager_type_tag_parses() {
        let pkg: Address = "0x22be4cade64bf2d02412c7e8d0e8beea2f78828b948118d46735315409371a3c"
            .parse()
            .unwrap();
        let tag = balance_manager_type_tag(pkg).unwrap();
        assert!(tag.to_string().contains("balance_manager"));
    }

    #[test]
    fn builds_create_register_share_ptb() {
        use sui_sdk_types::Address as SuiAddress;
        use sui_transaction_builder::TransactionBuilder;

        let pkg: SuiAddress = "0x22be4cade64bf2d02412c7e8d0e8beea2f78828b948118d46735315409371a3c"
            .parse()
            .unwrap();
        let registry = crate::chains::sui::execution::rpc_types::SharedObjectRef {
            object_id: "0x7c256edbda983a2cd6f946655f4bf3f00a41043993781f8674a7046e8c0e11d1"
                .parse()
                .unwrap(),
            initial_shared_version: 387_241_129,
            mutable: true,
        };
        let mut builder = TransactionBuilder::new();
        append_create_register_and_share(&mut builder, pkg, &registry).unwrap();
    }
}
