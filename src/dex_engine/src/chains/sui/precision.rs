//! Human price/qty ↔ on-chain tick/lot conversion for DeepBook pools.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use trading_core::venue::dex::DexError;

use super::types::DeepBookPoolMeta;

/// Convert human base quantity to lot count (must be an integer multiple of `lot_size`).
pub fn qty_to_lots(qty: Decimal, meta: &DeepBookPoolMeta) -> Result<u64, DexError> {
    let scale = decimal_scale(meta.base_asset_decimals);
    let raw = (qty * scale).round();
    let lot = Decimal::from(meta.lot_size);
    if lot.is_zero() {
        return Err(DexError::InvalidConfig("lot_size must be > 0".into()));
    }
    let rem = raw % lot;
    if !rem.is_zero() {
        return Err(DexError::InvalidConfig(
            format!(
                "quantity {qty} is not a multiple of lot size {}",
                lots_to_base_qty(1, meta)
            )
            .into(),
        ));
    }
    let lots = (raw / lot)
        .to_u64()
        .ok_or_else(|| DexError::InvalidConfig(format!("quantity {qty} out of range").into()))?;
    validate_min_lots(lots, meta)?;
    Ok(lots)
}

/// Convert lot count back to human base quantity.
pub fn lots_to_base_qty(lots: u64, meta: &DeepBookPoolMeta) -> Decimal {
    Decimal::from(lots * meta.lot_size) / decimal_scale(meta.base_asset_decimals)
}

/// Convert human price (quote per base) to on-chain tick count.
pub fn price_to_ticks(price: Decimal, meta: &DeepBookPoolMeta) -> Result<u64, DexError> {
    if meta.tick_size == 0 {
        return Err(DexError::InvalidConfig("tick_size must be > 0".into()));
    }
    let quote_scale = decimal_scale(meta.quote_asset_decimals);
    let raw = (price * quote_scale / Decimal::from(meta.tick_size)).round();
    raw.to_u64()
        .ok_or_else(|| DexError::InvalidConfig(format!("price {price} out of range").into()))
}

/// Convert on-chain tick count to human price.
pub fn ticks_to_price(ticks: u64, meta: &DeepBookPoolMeta) -> Decimal {
    Decimal::from(ticks * meta.tick_size) / decimal_scale(meta.quote_asset_decimals)
}

/// Human price → on-chain price units (must be a multiple of `tick_size`).
pub fn price_to_chain_units(price: Decimal, meta: &DeepBookPoolMeta) -> Result<u64, DexError> {
    let ticks = price_to_ticks(price, meta)?;
    Ok(ticks.saturating_mul(meta.tick_size))
}

/// Snap a human price to the nearest valid on-chain tick.
pub fn snap_price_to_tick(price: Decimal, meta: &DeepBookPoolMeta) -> Result<Decimal, DexError> {
    price_to_ticks(price, meta).map(|ticks| ticks_to_price(ticks, meta))
}

/// Ensure lot count satisfies pool `min_size` (in base smallest units).
pub fn validate_min_lots(lots: u64, meta: &DeepBookPoolMeta) -> Result<(), DexError> {
    let base_units = lots.saturating_mul(meta.lot_size);
    if base_units < meta.min_size {
        return Err(DexError::InvalidConfig(
            format!(
                "order size {} lots ({base_units} base units) below min_size {}",
                lots, meta.min_size
            )
            .into(),
        ));
    }
    Ok(())
}

fn decimal_scale(decimals: u32) -> Decimal {
    Decimal::from(10u64.pow(decimals))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use smol_str::SmolStr;

    fn sui_dbusdc_fixture() -> DeepBookPoolMeta {
        DeepBookPoolMeta {
            pool_id: SmolStr::new("0xpool"),
            pool_name: SmolStr::new("SUI_DBUSDC"),
            base_asset_id: SmolStr::new("sui"),
            base_asset_decimals: 9,
            base_asset_symbol: SmolStr::new("SUI"),
            base_asset_name: SmolStr::new("Sui"),
            quote_asset_id: SmolStr::new("dbusdc"),
            quote_asset_decimals: 6,
            quote_asset_symbol: SmolStr::new("DBUSDC"),
            quote_asset_name: SmolStr::new("Deepbook USDC"),
            min_size: 1_000_000_000,
            lot_size: 100_000_000,
            tick_size: 10,
        }
    }

    #[test]
    fn qty_lots_round_trip() {
        let meta = sui_dbusdc_fixture();
        let qty = dec!(1.0);
        let lots = qty_to_lots(qty, &meta).unwrap();
        assert_eq!(lots, 10);
        assert_eq!(lots_to_base_qty(lots, &meta), qty);
    }

    #[test]
    fn price_ticks_round_trip() {
        let meta = sui_dbusdc_fixture();
        let price = dec!(0.732);
        let ticks = price_to_ticks(price, &meta).unwrap();
        let back = ticks_to_price(ticks, &meta);
        assert_eq!(back, price);
    }

    #[test]
    fn chain_price_aligns_to_tick_grid() {
        let meta = sui_dbusdc_fixture();
        assert_eq!(price_to_chain_units(dec!(0.732), &meta).unwrap(), 732_000);
        // Binding mid + offset can land between ticks; must snap before PTB.
        assert_eq!(
            price_to_chain_units(dec!(0.704505), &meta).unwrap(),
            704_500
        );
    }

    #[test]
    fn rejects_below_min_size() {
        let meta = sui_dbusdc_fixture();
        assert!(qty_to_lots(dec!(0.5), &meta).is_err());
    }
}
