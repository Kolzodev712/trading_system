# fixed_long_0.001_btc — delayed_execution

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

Fixed +0.001 BTC long in the same linear perpetual. decision_time is the performance window start; there is no signal candle. Fees, execution adjustment, delay, funding, marks, and terminal treatment match the candidate run.

## Configuration
- scenario hash: `sha256:1c736b151f1836f1de841d7f098036d7bf439460830c3cdef7e66a7a2c8177d9`
- signal config hash: `benchmark-fixed-long`
- daily dataset hash: ``
- target fingerprint: `sha256:c092aaadb551e5d0b6b9b026a264a58363038435baa8b6843d634359b4eee6e1`
- fee_bps: 6 (synthetic)
- execution_adjustment_bps: 2 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 300 (synthetic)
- performance window: 2023-01-21T00:00:00Z inclusive start, 2026-01-01T00:00:00Z exclusive end

## Coverage
No auxiliary coverage notes on this run.

## Activity
- signal changes: 1
- fills: 1
- completed trades: 0
- pending executions: 0
- model direction at end: long
- simulated position at end: 0.001
- benchmark entry fill time: 2023-01-21T00:06:00Z

## PnL (USDT)
- gross price PnL: 64.92748710
- realized price PnL: 0
- unrealized price PnL: 64.92748710
- fees: 0.01360141974
- funding paid: 17.4615129807037
- funding received: 0.9181782755797
- signed funding: -16.5433347051240
- net PnL: 48.3705509751360
- estimated close cost (not in net PnL): 0.0700667044176
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 44.3887151092231 USDT, trough 2025-11-21T10:00:00Z, peak 2025-10-06T19:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 92966040000 ms (0.9999961276332094175960346964)
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
- open mark-to-market residual: 48.3705509751360
- largest winning trade: none
- largest three winning trades: none
- largest winner share of net: undefined_no_winning_trades
- largest three winners share of net: undefined_no_winning_trades

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 16.7981416192300 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
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
