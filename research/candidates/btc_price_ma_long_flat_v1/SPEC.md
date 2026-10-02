# BTCPriceMALongFlatV1

Legacy Candidate #2 **research** screen. This file describes the frozen
development experiment that was scored. It is **not** a master
preregistration. The spec was never committed to `master` as
`PREREGISTERED` before scoring.

The `strategy_core` model and `btc_price_ma_app` composition binary **remain
supported** for Paper / Demo observation even though this historical alpha
screen failed. The default runner factory does not construct this type;
use `cargo run -p btc_price_ma_app`.

## Frozen primary rule (as scored)

- Venue: Bybit mainnet, USDT linear perpetual, `BTCUSDT`, interval `D`.
- Long/flat only. Position `0.001` BTC.
- Primary lookback N=28 calendar-day geometric mean of daily closes.
- Diagnostics N=14 and N=70 (not replacements for primary).
- Performance window `[2023-01-21T00:00:00Z, 2026-01-01T00:00:00Z)` end exclusive.
- Warmup from 2022-11-01 is merged in front of the main daily series.
- Cost A is 6 bps + 2 bps + 10 s. Same simulator as Candidate #1.

Promotion gates (as stored in `gates.json`): Cost A net positive, retain 80%
of zero-cost profit, drawdown ≤ 70% of the fixed-long benchmark, beat
notional-matched passive, higher-cost and delayed nets positive, ≥ 20
completed round trips, N=14 and N=70 Cost A positive.
