# Platform data plane

## Layers

| Layer | Crate / module | Role |
|-------|----------------|------|
| **In-process** | `trading_core` broadcast channels | Source of truth inside the runner process: `MarketDataUpdate`, `CexVenueUpdate`, `DexVenueUpdate`, `IndicatorUpdate`. |
| **Observability** | `trading_trace` | Spans and optional `ColdTraceContext` on venue updates; Prometheus/Jaeger when exporters are enabled. |
| **External (opt-in)** | `trading_networking` | `HotEvent`, `ColdEvent`, `DexColdEvent` buses; Zenoh transport when `ZENOH_ENABLED=1`. |

Do **not** route internal trading through Zenoh — that adds serialization and latency.

## Runner bridge

`runner::event_bridge` subscribes to internal channels and publishes to networking buses using
`trading_networking::events::{hot_event_from_market_data, cold_event_from_cex, dex_cold_event_from_dex}`:

- MD → `HotEvent`
- CEX → `ColdEvent`
- DEX → `DexColdEvent` (topic prefix `trading/dex_cold/...` on Zenoh)

Used by `--mode levels`, `strategies`, and `dex` when external export is enabled.

Remote commands: see [COMMAND_PLANE.md](./COMMAND_PLANE.md). Zenoh smoke: [ZENOH_SMOKE.md](./ZENOH_SMOKE.md).

## Zenoh

- **Default: off.** Set `ZENOH_ENABLED=1` or `true` to start the event gateway and command queryable.
- Hot keys: `trading/hot/orderbook/{pair}`, `trading/hot/trade/{pair}`
- CEX cold keys: `trading/cold/order`, `balance`, `position`, `trade`
- DEX cold keys: `trading/dex_cold/wallet_balance`, `pending_transaction`, `execution_result`

## Trace

- **Hot path:** market data (orderbook/trade) — trace context on `MarketDataUpdate` / `HotEvent`.
- **Cold path:** CEX and DEX venue updates — `ColdTraceContext` on publish; ends when the update is sent on the broadcast channel.
