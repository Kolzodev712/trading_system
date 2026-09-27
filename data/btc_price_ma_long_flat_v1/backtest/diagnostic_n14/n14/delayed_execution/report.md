# btc_price_ma_long_flat_v1_n14 — delayed_execution

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

BTCPriceMALongFlatV1 with N=14 calendar-day geometric mean. Decisions come from offline replay; the simulator does not recompute the signal.

## Configuration
- scenario hash: `sha256:74ff30011cde0ade17d233725adffbbffb9a255fe3aea75f8137af6cbead4e7e`
- signal config hash: `sha256:7359e0e38ea3f855d1cf4d5438458c3d5b9805fb81b26ea07c223cd990d38c42`
- daily dataset hash: ``
- target fingerprint: `sha256:8e136196fadcaca2dc62bfcc594913d8140ff4ca829b862d3c9a1a4fb2b8bbe0`
- fee_bps: 6 (synthetic)
- execution_adjustment_bps: 2 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 300 (synthetic)
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
- pending 1767139200000-1767225600000 target 0 available 2026-01-01T00:05:00Z blocked fill minute 2026-01-01T00:06:00Z: available_at or the next one-minute open is outside the performance window; no fill was invented

## PnL (USDT)
- gross price PnL: 40.79299500
- realized price PnL: 41.64666150
- unrealized price PnL: -0.85366650
- fees: 6.047180374536
- funding paid: 11.8292336385416
- funding received: 0.3298599100614
- signed funding: -11.4993737284802
- net PnL: 23.2464408969838
- estimated close cost (not in net PnL): 0.0700667044176
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 34.1395496015525 USDT, trough 2025-04-12T03:00:00Z, peak 2024-11-22T22:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 50716440000 ms (0.5455351610904584882280049566)
- maximum marked or fill notional: 125.9719 USDT

## Trades
- winning / losing / breakeven: 19 / 59 / 0
- win rate: 0.2435897435897435897435897436
- average win: 6.2474448124778105263157894737
- average loss: -1.6022220412169932203389830508
- profit factor: 1.2556867622916647670118849276
- largest losing trade: -7.878511309077
- long net PnL: 24.1703510052758
- short net PnL: 0
- open mark-to-market residual: -0.923910108292
- largest winning trade: 21.9201881548896
- largest three winning trades: 55.6547928905714
- largest winner share of net: 0.9429481378258518779668475803
- largest three winners share of net: 2.3941210242550526447839200102

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 0.8130587843875 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): 28.9228737310455 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): -6.4894916184492 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

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
