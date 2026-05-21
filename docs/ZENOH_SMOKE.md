# Zenoh smoke test

Quick check that the external data plane publishes when Zenoh is enabled.

## Prerequisites

- Runner built with `zenoh` feature (default workspace runner package).
- [zenoh](https://zenoh.io/) CLI or API subscriber installed locally.
- Runner started with `ZENOH_ENABLED=1` (levels, strategies, or dex mode with market/venue activity).

## Subscribe (examples)

```bash
# Hot lane — orderbooks
zenoh subscribe 'trading/hot/orderbook/**'

# Hot lane — trades
zenoh subscribe 'trading/hot/trade/**'

# CEX cold lane
zenoh subscribe 'trading/cold/**'

# DEX cold lane (dex or both venue mode)
zenoh subscribe 'trading/dex_cold/**'
```

## Pass criteria

1. At least one JSON payload on a hot key after MD is flowing.
2. At least one cold or dex_cold payload after venue updates (CEX orders/balances or DEX wallet/execution).
3. Runner logs do not show repeated `Zenoh transport health` warnings with rising `publish_errors`.
4. Prometheus scrape (if enabled) shows `zenoh_transport_sent_total` increasing when `ZENOH_ENABLED=1`.

## Metrics

When Zenoh is on, platform metrics include:

- `zenoh_transport_sent_total`
- `zenoh_transport_dropped_total`
- `zenoh_transport_publish_errors_total`

See `runner::platform_metrics` and `docs/PLATFORM_DATA_PLANE.md`.
