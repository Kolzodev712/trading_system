# btc_price_ma_long_flat_v1_n14 — cost_a

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

BTCPriceMALongFlatV1 with N=14 calendar-day geometric mean. Decisions come from offline replay; the simulator does not recompute the signal.

## Configuration
- scenario hash: `sha256:625d05d6523748cc4e23badb2fc0b8fc646cedd98cb254afa2ead93d6e9057fb`
- signal config hash: `sha256:7359e0e38ea3f855d1cf4d5438458c3d5b9805fb81b26ea07c223cd990d38c42`
- daily dataset hash: ``
- target fingerprint: `sha256:8e136196fadcaca2dc62bfcc594913d8140ff4ca829b862d3c9a1a4fb2b8bbe0`
- fee_bps: 6 (synthetic)
- execution_adjustment_bps: 2 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 10 (synthetic)
- performance window: 2023-01-21T00:00:00Z inclusive start, 2026-01-01T00:00:00Z exclusive end

## Coverage
No auxiliary coverage notes on this run.

## Activity
- signal changes: 158
- fills: 157
- completed trades: 78
- pending executions: 1
- model direction at end: flat
- simulated position at end: 0.001
- pending 1767139200000-1767225600000 target 0 available 2026-01-01T00:00:10Z blocked fill minute 2026-01-01T00:01:00Z: available_at or the next one-minute open is outside the performance window; no fill was invented

## PnL (USDT)
- gross price PnL: 38.32612676
- realized price PnL: 39.15188768
- unrealized price PnL: -0.82576092
- fees: 6.046785390576
- funding paid: 11.8292336385416
- funding received: 0.3298599100614
- signed funding: -11.4993737284802
- net PnL: 20.7799676409438
- estimated close cost (not in net PnL): 0.0700667044176
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 35.2949480788581 USDT, trough 2025-04-10T16:00:00Z, peak 2024-11-22T22:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 50716740000 ms (0.5455383880627839735646427096)
- maximum marked or fill notional: 125.9719 USDT

## Trades
- winning / losing / breakeven: 19 / 59 / 0
- win rate: 0.2435897435897435897435897436
- average win: 6.2058528790603368421052631579
- average loss: -1.6311059199365864406779661017
- profit factor: 1.2252392505750519746780823163
- largest losing trade: -8.395869131141
- long net PnL: 21.6759554258878
- short net PnL: 0
- open mark-to-market residual: -0.895987784944
- largest winning trade: 21.5719138730816
- largest three winning trades: 55.1289873074154
- largest winner share of net: 1.0381110426070822696424293255
- largest three winners share of net: 2.6529871585936459485130996553

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 0.3305233664515 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): 27.8472076019135 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): -7.3977633274212 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

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
