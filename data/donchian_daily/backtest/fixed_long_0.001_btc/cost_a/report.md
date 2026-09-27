# fixed_long_0.001_btc — cost_a

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

Fixed +0.001 BTC long in the same linear perpetual. decision_time is the performance window start and there is no signal candle. Fees, execution adjustment, delay, funding, marks, and terminal treatment match the Donchian run. This comparison does not change the Donchian rules and is not a spot return.

## Configuration
- scenario hash: `sha256:8d60a72a99883fd12c3ce06628d51d644b7fa15c4dbc770d1289f48050734636`
- signal config hash: `sha256:8057b5024b6df11096bc7541e8f470e52794f5e2025503da45beedc0aab33114`
- daily dataset hash: `sha256:bb642e2c001a0df039b15fbad7a7032cab61ae41f29eef785823facb55265236`
- target fingerprint: `sha256:c092aaadb551e5d0b6b9b026a264a58363038435baa8b6843d634359b4eee6e1`
- fee_bps: 6 (synthetic)
- execution_adjustment_bps: 2 (synthetic combined spread/slippage; already inside fill prices)
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
- signal changes: 1
- fills: 1
- completed trades: 0
- pending executions: 0
- model direction at end: long
- simulated position at end: 0.001
- benchmark entry fill time: 2023-01-21T00:01:00Z

## PnL (USDT)
- gross price PnL: 64.91748510
- realized price PnL: 0
- unrealized price PnL: 64.91748510
- fees: 0.01360742094
- funding paid: 17.4615129807037
- funding received: 0.9181782755797
- signed funding: -16.5433347051240
- net PnL: 48.3605429739360
- estimated close cost (not in net PnL): 0.0700667044176
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 44.3887151092231 USDT, trough 2025-11-21T10:00:00Z, peak 2025-10-06T19:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 92966340000 ms (0.9999993546055349029326724494)
- maximum marked or fill notional: 125.9719 USDT

## Trades
- winning / losing / breakeven: 0 / 0 / 0
- win rate: undefined_no_trades
- average win: undefined_no_winning_trades
- average loss: undefined_no_losing_trades
- profit factor: undefined_no_trades
- largest losing trade: undefined_no_losing_trades
- long net PnL: 0
- short net PnL: 0
- open mark-to-market residual: 48.3605429739360
- largest winning trade: none
- largest three winning trades: none
- largest winner share of net: undefined_no_winning_trades
- largest three winners share of net: undefined_no_winning_trades

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 16.7881336180300 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): 42.8494990905739 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): -11.2770897346679 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

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
