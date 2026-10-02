# Live strategies versus rejected research models

These decisions are for HEAD after the legacy archive cleanup.

| Name | Role | Decision |
| --- | --- | --- |
| `DonchianDailyV1` | Rejected research candidate | Archived. Not a live factory type. |
| `Donchian` | Runner factory + daily warmup | **Keep.** Selectable. Not the same implementation as DonchianDailyV1. |
| `BuyLowSellHigh` | Runner factory; sample_config for binance/bybit/k-paper | **Keep.** Still the default sample CEX strategy. |
| `BTCPriceMALongFlatV1` | Rejected *research screen*; live model in `strategy_core` | Research rejected. **Keep** model + `btc_price_ma_app` for Demo/Paper. Default runner factory still refuses it. |
| `src/strategy_engine` IR scaffold | Unused workspace member | Removed from the compiled workspace. See that directory's README. |
