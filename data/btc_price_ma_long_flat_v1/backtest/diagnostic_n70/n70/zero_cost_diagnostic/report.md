# btc_price_ma_long_flat_v1_n70 — zero_cost_diagnostic

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

BTCPriceMALongFlatV1 with N=70 calendar-day geometric mean. Decisions come from offline replay; the simulator does not recompute the signal.

## Configuration
- scenario hash: `sha256:03837549b1047cf052836b795da729f7559cc89cdb051c868c8687c704336991`
- signal config hash: `sha256:0b0d37fef00edddb81fd1b5eff79f3c5912b5290842dab499341b6016c045875`
- daily dataset hash: ``
- target fingerprint: `sha256:ef299181ad19a8b65057cc7e135319c18ab281396266da725ac212cf8749ad8e`
- fee_bps: 0 (synthetic)
- execution_adjustment_bps: 0 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 10 (synthetic)
- performance window: 2023-01-21T00:00:00Z inclusive start, 2026-01-01T00:00:00Z exclusive end

## Coverage
No auxiliary coverage notes on this run.

## Activity
- signal changes: 46
- fills: 46
- completed trades: 23
- pending executions: 0
- model direction at end: flat
- simulated position at end: 0

## PnL (USDT)
- gross price PnL: 58.3982
- realized price PnL: 58.3982
- unrealized price PnL: 0
- fees: 0
- funding paid: 13.9136314787635
- funding received: 0.3710825082352
- signed funding: -13.5425489705283
- net PnL: 44.8556510294717
- estimated close cost (not in net PnL): 0
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 30.6627554544984 USDT, trough 2024-10-10T19:00:00Z, peak 2024-03-14T07:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 60393600000 ms (0.649628252788104089219330855)
- maximum marked or fill notional: 125.9719 USDT

## Trades
- winning / losing / breakeven: 8 / 15 / 0
- win rate: 0.3478260869565217391304347826
- average win: 10.7422752455567875
- average loss: -2.738836728998840
- profit factor: 2.0918419136255784708929025774
- largest losing trade: -7.054547141
- long net PnL: 44.8556510294717
- short net PnL: 0
- open mark-to-market residual: 0
- largest winning trade: 28.5296649681569
- largest three winning trades: 67.7041174491929
- largest winner share of net: 0.6360327921539235348401745804
- largest three winners share of net: 1.5093776568911902428837764169

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 14.5616239048924 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): 27.0590804624245 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): 3.2349466621548 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

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
