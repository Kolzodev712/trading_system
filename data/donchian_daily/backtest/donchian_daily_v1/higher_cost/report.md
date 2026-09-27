# donchian_daily_v1 — higher_cost

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

DonchianDailyV1 decisions from the frozen replay. The simulator does not recompute channels.

## Configuration
- scenario hash: `sha256:3ab5adcc5ce51a97435b04b00b6508a3623f134f762c1384cf2d86d5064fddb6`
- signal config hash: `sha256:8057b5024b6df11096bc7541e8f470e52794f5e2025503da45beedc0aab33114`
- daily dataset hash: `sha256:bb642e2c001a0df039b15fbad7a7032cab61ae41f29eef785823facb55265236`
- target fingerprint: `sha256:efcc093776006ec556a8b66e8846bf175f5e73b5ca0021f666ba5c8173f7f854`
- fee_bps: 10 (synthetic)
- execution_adjustment_bps: 10 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 10 (synthetic)
- performance window: 2023-01-21T00:00:00Z inclusive start, 2026-01-01T00:00:00Z exclusive end

## Coverage
- trade_1m hash `sha256:d68ed735b4c06eb6f758b0a9afa24859d2f23276f4ca4c93fa62f8575f578d84`, endpoint `/v5/market/kline`, environment mainnet, count 354, pages 59, gaps 0, pagination_complete true, span 2023-01-21T00:01:00Z .. 2025-12-04T00:06:00Z
  observed intervals ms: [60000, 86100000, 172500000, 258900000, 345300000, 431700000, 518100000, 604500000, 690900000, 863700000, 1036500000, 1122900000, 1209300000, 1295700000, 1382100000, 1554900000, 1641300000, 1727700000, 1814100000, 1900500000, 1986900000, 2073300000, 2159700000, 2332500000, 2591700000, 2850900000, 3282900000, 3542100000, 3714900000, 3887700000, 7862100000, 8380500000]
- funding hash `sha256:a461acf4b952fe2e59e2166610707ef21cbcee0c88b92eb474f816bfa24e361f`, endpoint `/v5/market/funding/history`, environment mainnet, count 3235, pages 17, gaps 0, pagination_complete true, span 2023-01-20T00:00:00Z .. 2026-01-02T00:00:00Z
  observed intervals ms: [28800000]
- mark_60 hash `sha256:993ea03c9d15acf1b9c305e5686e4cd5c4e7b243b83d559a61d7cb96a3febccd`, endpoint `/v5/market/mark-price-kline`, environment mainnet, count 25825, pages 26, gaps 0, pagination_complete true, span 2023-01-21T00:00:00Z .. 2026-01-01T00:00:00Z
  observed intervals ms: [3600000]

## Activity
- signal changes: 58
- fills: 58
- completed trades: 29
- pending executions: 0
- model direction at end: flat
- simulated position at end: 0

## PnL (USDT)
- gross price PnL: -33.2177682
- realized price PnL: -33.2177682
- unrealized price PnL: 0
- fees: 3.9313974864
- funding paid: 11.1861170415345
- funding received: 2.2147383300875
- signed funding: -8.9713787114470
- net PnL: -46.1205443978470
- estimated close cost (not in net PnL): 0
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 71.3181440906912 USDT, trough 2025-11-16T10:00:00Z, peak 2025-03-10T19:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 59097600000 ms (0.6356877323420074349442379182)
- maximum marked or fill notional: 125.9719 USDT

## Trades
- winning / losing / breakeven: 9 / 20 / 0
- win rate: 0.3103448275862068965517241379
- average win: 7.2036243902378666666666666667
- average loss: -5.547658195499390
- profit factor: 0.5843242069666179810998504832
- largest losing trade: -19.7606519596323
- long net PnL: 18.1789835688028
- short net PnL: -64.2995279666498
- open mark-to-market residual: 0
- largest winning trade: 24.0778932599441
- largest three winning trades: 49.0993843455008
- largest winner share of net: -0.5220643766093121962983561217
- largest three winners share of net: -1.0645881349959272869000703177

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): -4.3272785368455 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): 8.9279567681872 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): -50.7212226291887 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

## Limitations
- full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness
- prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency
- No capital, borrowing, margin, or liquidation model. Figures are futures PnL in USDT, not percentage account returns and not validated leveraged performance.
- The execution-price adjustment is included in the fill price and is not subtracted again.
- Equal timestamps apply funding to the pre-fill position, then the fill, then the mark sample. A fill does not pay a settlement timestamped before it.
- An hourly funding settlement uses that hourly mark candle's open. This is the labelled settlement-mark proxy hourly_mark_open_at_settlement, not a trade print.
- A position still open at the cutoff is marked to market. estimated_close_cost_usdt is disclosed and is not included in net PnL. The window is not extended to finish the trade.
- No-change keeps the existing target and does not flatten or submit another entry.
- The fixed long comparison uses the same perpetual, funding, and execution assumptions. It is not an unrelated spot return.
