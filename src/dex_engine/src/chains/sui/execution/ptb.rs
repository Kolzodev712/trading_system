//! DeepBook V3 PTB builders (mirrors `@mysten/deepbook-v3` move calls).

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sui_sdk_types::{Address, Identifier, TypeTag};
use sui_transaction_builder::{Argument, Function, ObjectInput, TransactionBuilder};
use trading_core::types::common::Side;
use trading_core::venue::dex::DexError;

use super::super::precision::price_to_chain_units;
use super::super::types::DeepBookPoolMeta;

/// Order expiration sentinel (TS SDK `MAX_TIMESTAMP`).
pub const MAX_TIMESTAMP: u64 = 1_844_674_407_370_955_161;
/// Default gas budget in MIST (0.25 SUI).
pub const GAS_BUDGET: u64 = 250_000_000;
/// Sui system `Clock` shared object.
pub const CLOCK_OBJECT_ID: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000006";

const ORDER_TYPE_NO_RESTRICTION: u8 = 0;
const SELF_MATCHING_ALLOWED: u8 = 0;

pub struct DeepBookPtbContext {
    pub package_id: Address,
    pub pool: DeepBookPoolMeta,
    pub balance_manager: ObjectInput,
    pub pool_object: ObjectInput,
    pub clock: ObjectInput,
}

impl DeepBookPtbContext {
    pub fn base_type(&self) -> Result<TypeTag, DexError> {
        parse_type_tag(&self.pool.base_asset_id)
    }

    pub fn quote_type(&self) -> Result<TypeTag, DexError> {
        parse_type_tag(&self.pool.quote_asset_id)
    }

    fn pool_fn(&self, name: &str) -> Result<Function, DexError> {
        Ok(Function::new(
            self.package_id,
            Identifier::new("pool").map_err(id_err)?,
            Identifier::new(name).map_err(id_err)?,
        )
        .with_type_args(vec![self.base_type()?, self.quote_type()?]))
    }

    fn balance_manager_fn(&self, name: &str) -> Result<Function, DexError> {
        Ok(Function::new(
            self.package_id,
            Identifier::new("balance_manager").map_err(id_err)?,
            Identifier::new(name).map_err(id_err)?,
        ))
    }

    pub fn generate_trade_proof(&self, tx: &mut TransactionBuilder) -> Result<Argument, DexError> {
        let manager = tx.object(self.balance_manager.clone());
        Ok(tx.move_call(
            self.balance_manager_fn("generate_proof_as_owner")?,
            vec![manager],
        ))
    }

    pub fn append_place_limit_order(
        &self,
        tx: &mut TransactionBuilder,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        pay_fees_in_deep: bool,
        client_order_id: u64,
    ) -> Result<(), DexError> {
        let func = self.pool_fn("place_limit_order")?;
        let trade_proof = self.generate_trade_proof(tx)?;
        let pool = tx.object(self.pool_object.clone());
        let manager = tx.object(self.balance_manager.clone());
        let chain_price = price_to_chain_units(price, &self.pool)?;
        let chain_qty = quantity_to_base_units(quantity, &self.pool)?;
        let is_bid = matches!(side, Side::Buy);
        let arg_client_id = tx.pure(&client_order_id);
        let arg_order_type = tx.pure(&ORDER_TYPE_NO_RESTRICTION);
        let arg_self_match = tx.pure(&SELF_MATCHING_ALLOWED);
        let arg_price = tx.pure(&chain_price);
        let arg_qty = tx.pure(&chain_qty);
        let arg_is_bid = tx.pure(&is_bid);
        let arg_pay_deep = tx.pure(&pay_fees_in_deep);
        let arg_expiry = tx.pure(&MAX_TIMESTAMP);
        let arg_clock = tx.object(self.clock.clone());
        tx.move_call(
            func,
            vec![
                pool,
                manager,
                trade_proof,
                arg_client_id,
                arg_order_type,
                arg_self_match,
                arg_price,
                arg_qty,
                arg_is_bid,
                arg_pay_deep,
                arg_expiry,
                arg_clock,
            ],
        );
        Ok(())
    }

    pub fn append_cancel_order(
        &self,
        tx: &mut TransactionBuilder,
        order_id: &str,
    ) -> Result<(), DexError> {
        let func = self.pool_fn("cancel_order")?;
        let trade_proof = self.generate_trade_proof(tx)?;
        let pool = tx.object(self.pool_object.clone());
        let manager = tx.object(self.balance_manager.clone());
        let order_id_u128: u128 = order_id.parse().map_err(|e| {
            DexError::InvalidConfig(format!("invalid order_id `{order_id}`: {e}").into())
        })?;
        let arg_order_id = tx.pure(&order_id_u128);
        let arg_clock = tx.object(self.clock.clone());
        tx.move_call(
            func,
            vec![pool, manager, trade_proof, arg_order_id, arg_clock],
        );
        Ok(())
    }

    pub fn append_cancel_all_orders(&self, tx: &mut TransactionBuilder) -> Result<(), DexError> {
        let func = self.pool_fn("cancel_all_orders")?;
        let trade_proof = self.generate_trade_proof(tx)?;
        let pool = tx.object(self.pool_object.clone());
        let manager = tx.object(self.balance_manager.clone());
        let arg_clock = tx.object(self.clock.clone());
        tx.move_call(func, vec![pool, manager, trade_proof, arg_clock]);
        Ok(())
    }

    pub fn append_deposit(
        &self,
        tx: &mut TransactionBuilder,
        coin_type: &str,
        amount: Decimal,
        coin_object: ObjectInput,
    ) -> Result<(), DexError> {
        let type_tag = parse_type_tag(coin_type)?;
        let chain_amount = amount_to_coin_units(amount, coin_type, &self.pool)?;
        let manager = tx.object(self.balance_manager.clone());
        let amount_arg = tx.pure(&chain_amount);
        let coin_arg = tx.object(coin_object);
        let split = tx.split_coins(coin_arg, vec![amount_arg]);
        tx.move_call(
            self.balance_manager_fn("deposit")?
                .with_type_args(vec![type_tag]),
            vec![manager, split.into_iter().next().expect("split coin")],
        );
        Ok(())
    }

    pub fn append_withdraw(
        &self,
        tx: &mut TransactionBuilder,
        coin_type: &str,
        amount: Decimal,
        recipient: Address,
    ) -> Result<(), DexError> {
        let func = self
            .balance_manager_fn("withdraw")?
            .with_type_args(vec![parse_type_tag(coin_type)?]);
        let chain_amount = amount_to_coin_units(amount, coin_type, &self.pool)?;
        let manager = tx.object(self.balance_manager.clone());
        let arg_amount = tx.pure(&chain_amount);
        let withdrawn = tx.move_call(func, vec![manager, arg_amount]);
        let arg_recipient = tx.pure(&recipient);
        tx.transfer_objects(vec![withdrawn], arg_recipient);
        Ok(())
    }
}

pub fn quantity_to_base_units(qty: Decimal, meta: &DeepBookPoolMeta) -> Result<u64, DexError> {
    let base_scale = decimal_scale(meta.base_asset_decimals);
    let scaled = (qty * base_scale).round();
    scaled
        .to_u64()
        .ok_or_else(|| DexError::InvalidConfig(format!("quantity {qty} out of range").into()))
}

pub fn coin_types_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    match (parse_type_tag(left), parse_type_tag(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn amount_to_coin_units(
    amount: Decimal,
    coin_type: &str,
    meta: &DeepBookPoolMeta,
) -> Result<u64, DexError> {
    let decimals = if coin_types_match(coin_type, meta.base_asset_id.as_str()) {
        meta.base_asset_decimals
    } else if coin_types_match(coin_type, meta.quote_asset_id.as_str()) {
        meta.quote_asset_decimals
    } else {
        return Err(DexError::InvalidConfig(
            format!("coin type `{coin_type}` does not match pool assets").into(),
        ));
    };
    let scaled = (amount * decimal_scale(decimals)).round();
    scaled
        .to_u64()
        .ok_or_else(|| DexError::InvalidConfig(format!("amount {amount} out of range").into()))
}

fn decimal_scale(decimals: u32) -> Decimal {
    Decimal::from(10u64.pow(decimals))
}

pub fn parse_address(s: &str) -> Result<Address, DexError> {
    s.parse()
        .map_err(|e| DexError::InvalidConfig(format!("invalid Sui address `{s}`: {e}").into()))
}

pub fn parse_type_tag(s: &str) -> Result<TypeTag, DexError> {
    s.parse()
        .map_err(|e| DexError::InvalidConfig(format!("invalid type tag `{s}`: {e}").into()))
}

pub(crate) fn id_err(e: impl std::fmt::Display) -> DexError {
    DexError::InvalidConfig(format!("invalid Move identifier: {e}").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;

    fn sui_dbusdc_fixture() -> DeepBookPoolMeta {
        DeepBookPoolMeta {
            pool_id: SmolStr::new(
                "0x1c19362ca52b8ffd7a33cee805a67d40f31e6ba303753fd3a4cfdfacea7163a5",
            ),
            pool_name: SmolStr::new("SUI_DBUSDC"),
            base_asset_id: SmolStr::new(
                "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
            ),
            base_asset_decimals: 9,
            base_asset_symbol: SmolStr::new("SUI"),
            base_asset_name: SmolStr::new("Sui"),
            quote_asset_id: SmolStr::new(
                "0xf7152c05930480cd740d7311b5b8b45c6f488e3a53a11c3f74a6fac36a52e0d7::DBUSDC::DBUSDC",
            ),
            quote_asset_decimals: 6,
            quote_asset_symbol: SmolStr::new("DBUSDC"),
            quote_asset_name: SmolStr::new("Deepbook USDC"),
            min_size: 1_000_000_000,
            lot_size: 100_000_000,
            tick_size: 10,
        }
    }

    #[test]
    fn price_matches_ts_sdk_formula() {
        let meta = sui_dbusdc_fixture();
        let price = dec!(0.732);
        assert_eq!(price_to_chain_units(price, &meta).unwrap(), 732_000);
    }

    #[test]
    fn quantity_in_base_units() {
        let meta = sui_dbusdc_fixture();
        assert_eq!(
            quantity_to_base_units(dec!(1.0), &meta).unwrap(),
            1_000_000_000
        );
    }
}
