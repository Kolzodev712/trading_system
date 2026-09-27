# DonchianDailyV1 compared with a fixed +0.001 BTC perpetual long

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness**

Both books use the same historical funding, fees, execution adjustment, delay, marks, and terminal treatment. The fixed long is not a spot-price return and does not change the Donchian rules. No percentage account return is reported because no capital is defined.

| scenario | book | net PnL USDT | max drawdown USDT | drawdown trough | time in market ms | max notional USDT | fees | funding | ending position | entry |
| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| zero_cost_diagnostic | donchian_daily_v1 | -38.2577787114470 | 67.0729369558912 | 2025-11-16T10:00:00Z | 59097600000 | 125.9719 | 0 | -8.9713787114470 | 0 | - |
| zero_cost_diagnostic | fixed_long_0.001_btc | 48.3786852948760 | 44.3887151092231 | 2025-11-21T10:00:00Z | 92966340000 | 125.9719 | 0 | -16.5433347051240 | 0.001 | 2023-01-21T00:01:00Z |
| cost_a | donchian_daily_v1 | -41.4028767858150 | 68.7710066120672 | 2025-11-16T10:00:00Z | 59097600000 | 125.9719 | 2.358824434368 | -8.9713787114470 | 0 | - |
| cost_a | fixed_long_0.001_btc | 48.3605429739360 | 44.3887151092231 | 2025-11-21T10:00:00Z | 92966340000 | 125.9719 | 0.01360742094 | -16.5433347051240 | 0.001 | 2023-01-21T00:01:00Z |
| higher_cost | donchian_daily_v1 | -46.1205443978470 | 71.3181440906912 | 2025-11-16T10:00:00Z | 59097600000 | 125.9719 | 3.9313974864 | -8.9713787114470 | 0 | - |
| higher_cost | fixed_long_0.001_btc | 48.3333136203760 | 44.3887151092231 | 2025-11-21T10:00:00Z | 92966340000 | 125.9719 | 0.0226971745 | -16.5433347051240 | 0.001 | 2023-01-21T00:01:00Z |
| delayed_execution | donchian_daily_v1 | -43.0533657038030 | 70.1868741419392 | 2025-11-16T10:00:00Z | 59097600000 | 125.9719 | 2.359266172356 | -8.9713787114470 | 0 | - |
| delayed_execution | fixed_long_0.001_btc | 48.3705509751360 | 44.3887151092231 | 2025-11-21T10:00:00Z | 92966040000 | 125.9719 | 0.01360141974 | -16.5433347051240 | 0.001 | 2023-01-21T00:06:00Z |
