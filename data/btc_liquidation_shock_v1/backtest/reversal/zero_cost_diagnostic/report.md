# reversal — zero_cost_diagnostic

**prescribed synthetic research inputs, not verified account fees, measured slippage, or measured production latency**

**Fill assumption.** full fill: the entire required quantity fills at one adverse-adjusted one-minute open. This does not verify IOC fillability, liquidity, partial fills, historical exchange filters, or live execution correctness

BTCLiquidationShockV1 target changes from the frozen hourly rule

## Configuration
- scenario hash: `sha256:0032e21ae8604b693c4d330e27543f99ec8c190deb663ee1c5bd935f7d17e59c`
- signal config hash: `sha256:5df3141260c7d66b8685dcd7910883cd7e29b8a26b468e6cdcd8c68d4ebaca34`
- daily dataset hash: `sha256:806a91c0afedee338bd2435a753e9a862e10279c9a9d12e969aabcfa839e415a`
- target fingerprint: `sha256:31c85fb18299ebc752b172df19b80be75e11dc5ee8bed57bfe61dc81e23d0f95`
- fee_bps: 0 (synthetic)
- execution_adjustment_bps: 0 (synthetic combined spread/slippage; already inside fill prices)
- decision_delay_seconds: 10 (synthetic)
- performance window: 2023-01-21T00:00:00Z inclusive start, 2026-01-01T00:00:00Z exclusive end

## Coverage
- trade_1m hash `sha256:3a1941cd9facb31fed335172660c9b814521edef2c8b03bef1dc07fea2e84c4c`, endpoint `/v5/market/kline`, environment mainnet, count 4998, pages 833, gaps 0, pagination_complete true, span 2023-01-21T00:01:00Z .. 2025-12-29T10:06:00Z
  observed intervals ms: [60000, 3300000, 6900000, 10500000, 14100000, 17700000, 21300000, 24900000, 28500000, 32100000, 35700000, 39300000, 42900000, 46500000, 50100000, 53700000, 57300000, 60900000, 64500000, 68100000, 71700000, 75300000, 78900000, 82500000, 86100000, 89700000, 93300000, 96900000, 100500000, 104100000, 107700000, 114900000, 118500000, 122100000, 125700000, 129300000, 132900000, 136500000, 140100000, 143700000, 147300000, 150900000, 154500000, 158100000, 165300000, 168900000, 172500000, 176100000, 179700000, 183300000, 186900000, 190500000, 194100000, 197700000, 204900000, 208500000, 212100000, 215700000, 219300000, 222900000, 233700000, 237300000, 240900000, 244500000, 248100000, 251700000, 255300000, 258900000, 262500000, 266100000, 269700000, 273300000, 276900000, 284100000, 291300000, 294900000, 298500000, 302100000, 305700000, 309300000, 316500000, 320100000, 323700000, 327300000, 330900000, 334500000, 338100000, 345300000, 348900000, 363300000, 366900000, 370500000, 374100000, 377700000, 384900000, 388500000, 392100000, 395700000, 399300000, 402900000, 406500000, 410100000, 417300000, 420900000, 424500000, 431700000, 435300000, 438900000, 442500000, 449700000, 456900000, 467700000, 474900000, 485700000, 492900000, 500100000, 503700000, 507300000, 510900000, 518100000, 528900000, 543300000, 554100000, 557700000, 568500000, 572100000, 575700000, 579300000, 582900000, 590100000, 597300000, 600900000, 608100000, 629700000, 644100000, 658500000, 665700000, 683700000, 708900000, 734100000, 820500000, 842100000, 874500000, 888900000, 928500000, 932100000, 986100000, 1079700000, 1400100000]
- funding hash `sha256:a461acf4b952fe2e59e2166610707ef21cbcee0c88b92eb474f816bfa24e361f`, endpoint `/v5/market/funding/history`, environment mainnet, count 3235, pages 17, gaps 0, pagination_complete true, span 2023-01-20T00:00:00Z .. 2026-01-02T00:00:00Z
  observed intervals ms: [28800000]
- mark_60 hash `sha256:993ea03c9d15acf1b9c305e5686e4cd5c4e7b243b83d559a61d7cb96a3febccd`, endpoint `/v5/market/mark-price-kline`, environment mainnet, count 25825, pages 26, gaps 0, pagination_complete true, span 2023-01-21T00:00:00Z .. 2026-01-01T00:00:00Z
  observed intervals ms: [3600000]

## Activity
- signal changes: 832
- fills: 832
- completed trades: 416
- pending executions: 0
- model direction at end: flat
- simulated position at end: 0

## PnL (USDT)
- gross price PnL: -14.0291
- realized price PnL: -14.0291
- unrealized price PnL: 0
- fees: 0
- funding paid: 0.8262017270269
- funding received: 0.8856547032706
- signed funding: 0.0594529762437
- net PnL: -13.9696470237563
- estimated close cost (not in net PnL): 0
- Disclosed only. Not included in net PnL. A remaining position is not force-closed.
- max sampled drawdown: 33.0935449456162 USDT, trough 2025-11-21T09:00:00Z, peak 2024-11-05T20:00:00Z
- drawdown sampling: hourly mark opens from the performance start through the performance end inclusive, plus a sample after each fill and each funding cash flow; drawdown is the decline in cumulative net PnL from its running peak, in USDT
- time in market: 8985600000 ms (0.0966542750929368029739776952)
- maximum marked or fill notional: 125.1402 USDT

## Trades
- winning / losing / breakeven: 218 / 198 / 0
- win rate: 0.5240384615384615384615384615
- average win: 0.7091183104542688073394495413
- average loss: -0.8513001954686207070707070707
- profit factor: 0.9171223344916350568988876209
- largest losing trade: -6.424
- long net PnL: -9.1771583749026
- short net PnL: -4.7924886488537
- open mark-to-market residual: 0
- largest winning trade: 4.36564905
- largest three winning trades: 11.752558084003
- largest winner share of net: -0.3125096176428744241275804835
- largest three winners share of net: -0.8412924151925245496154504865

## Calendar-year PnL
Yearly figures are marked cumulative changes, including costs while a trade is open. An event exactly at 1 January belongs to the year that is starting. Years sum to net PnL.
- 2023 (partial): 3.1076274133502 USDT, 2023-01-21T00:00:00Z .. 2024-01-01T00:00:00Z
  coverage is 2023-01-21T00:00:00Z through 2024-01-01T00:00:00Z, not the full calendar year
- 2024 (full): -9.9436829552803 USDT, 2024-01-01T00:00:00Z .. 2025-01-01T00:00:00Z
- 2025 (full): -7.1335914818262 USDT, 2025-01-01T00:00:00Z .. 2026-01-01T00:00:00Z

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
