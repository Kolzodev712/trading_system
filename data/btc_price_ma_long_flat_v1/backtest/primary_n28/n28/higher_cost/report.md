# btc_price_ma_long_flat_v1_n28 — higher_cost

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

BTCPriceMALongFlatV1 with N=28 calendar-day geometric mean. Decisions come from offline replay; the simulator does not recompute the signal.

## Configuration
- scenario hash: `sha256:46dd2563fa0a972abe88dcd42ffa1473b6d36e926e73323bb7cffa7b7b00785e`
- signal config hash: `sha256:6e02ddef41a418dc26f5ea8acfd592417846362211b3f70de3ab553d41c3559a`
- daily dataset hash: ``
- target fingerprint: `sha256:af0e90cddec2f9cd36ae4222b56c1e419b9783f891391f48b24e9c5071c60429`
- fee_bps: 10 (synthetic)
- execution_adjustment_bps: 10 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 10 (synthetic)
- performance window: 2023-01-21T00:00:00Z inclusive start, 2026-01-01T00:00:00Z exclusive end

## Coverage
No auxiliary coverage notes on this run.

## Activity
- signal changes: 110
- fills: 110
- completed trades: 55
- pending executions: 0
- model direction at end: flat
- simulated position at end: 0

## PnL (USDT)
- gross price PnL: 31.2316470
- realized price PnL: 31.2316470
- unrealized price PnL: 0
- fees: 7.3041144642
- funding paid: 12.4070765205769
- funding received: 0.3266644255868
- signed funding: -12.0804120949901
- net PnL: 11.8471204408099
- estimated close cost (not in net PnL): 0
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 32.7499621760885 USDT, trough 2025-12-15T00:01:00Z, peak 2025-05-22T04:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 51062400000 ms (0.54925650557620817843866171)
- maximum marked or fill notional: 125.9719 USDT

## Trades
- winning / losing / breakeven: 12 / 43 / 0
- win rate: 0.2181818181818181818181818182
- average win: 7.9131411687446416666666666667
- average loss: -1.9328040368401348837209302326
- profit factor: 1.1425464887306795277780221332
- largest losing trade: -5.1448590059572
- long net PnL: 11.8471204408099
- short net PnL: 0
- open mark-to-market residual: 0
- largest winning trade: 31.8806413161832
- largest three winning trades: 67.9545259757107
- largest winner share of net: 2.6910033940706486437257982045
- largest three winners share of net: 5.7359529950946587717202979546

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 9.3037805652492 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): 19.8085972122956 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): -17.2652573367349 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

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
