# Independent audit: DonchianDailyV1 frozen backtest

Checker: `audit/donchian_daily_v1/reference_check.py`.

Command, from the repository root:

```text
python3 audit/donchian_daily_v1/reference_check.py
```

The checker reads stored JSON only. It does not call the strategy model, the replay, or the backtest accounting code. Comparison tolerances are exact `Decimal` equality. No rounding band is applied. Trailing zeros are not a difference.

Audited: zero-trading-cost diagnostic (fee 0, adverse adjustment 0, delay 10 seconds, historical funding included) and Cost A (fee 6 bps, adverse adjustment 2 bps, delay 10 seconds, same funding). Cost A fixed-long comparator is included.

Not audited: `higher_cost` and `delayed_execution` for both books. Their files were not used as evidence.

## Identity

- daily candles: 1096 model rows from the stored series; dataset hash `sha256:bb642e2c001a0df039b15fbad7a7032cab61ae41f29eef785823facb55265236`
- signal config hash `sha256:8057b5024b6df11096bc7541e8f470e52794f5e2025503da45beedc0aab33114`
- trade-1m points stored: 354. This is a sparse tape of required minutes, not a continuous three-year tape.
- funding records stored: 3235. Observed successive gap is 8 hours on every pair in the file.
- hourly marks stored: 25825. The inclusive performance grid is checked hour by hour.
- performance window: 2023-01-21T00:00:00Z inclusive through 2026-01-01T00:00:00Z exclusive.

## Decisions

Independent count: 1096 candle decisions, of which 20 are warm-up. Target changes: 58. Entries (non-zero targets): 29. Exits (zero targets): 29. Final direction: flat.

Compared with the recorded log: 1096 aligned candles. Economic decision mismatches: 0. Reason-label mismatches: 0.

Rules applied: prior 20 daily highs/lows for entry, prior 10 for exit, current candle excluded, strict inequalities, shorting enabled, quantity 0.001 BTC, no same-candle reversal, no resize. The 20 candles immediately before 2023-01-21 are warm-up and produce no target.

## Individual trades

Ranking scenario: zero-trading-cost diagnostic. Trade net includes historical funding and, in this scenario, zero fees and zero adverse adjustment. Cost A is reconciled in the ledgers but is not the ranking scenario.

### Largest losing trade

### long 0.001 BTC, net -19.3167411596323 USDT

Selected as the minimum zero-trading-cost trade net.

- entry signal candle 2025-10-02T00:00:00Z to 2025-10-03T00:00:00Z, close 120484
- previous direction flat; decision close above entry high; new direction long
- entry channel window 2025-09-12T00:00:00Z inclusive to 2025-10-02T00:00:00Z exclusive
- entry high 118644 from daily starts 1759276800000
- entry low 108550 from daily starts 1758844800000
- exit channel at entry: high 118644, low 108550
- exit signal candle 2025-11-04T00:00:00Z to 2025-11-05T00:00:00Z, close 101441.9
- exit reason close below exit low
- exit channel window 2025-10-25T00:00:00Z inclusive to 2025-11-04T00:00:00Z exclusive
- exit high 116373.3 from daily starts 1761523200000
- exit low 105178.2 from daily starts 1762128000000
- entry fill 2025-10-03T00:01:00Z price 120373
- exit fill 2025-11-05T00:01:00Z price 101573
- price PnL = 0.001 * (101573 - 120373) = -18.8
- fees 0; funding on 99 settlements summing to -0.5167411596323
- trade funding field -0.5167411596323; net = price - fees + funding = -19.3167411596323

### Largest winning trade

### long 0.001 BTC, net 24.4131365042441 USDT

Selected as the maximum zero-trading-cost trade net.

- entry signal candle 2024-10-28T00:00:00Z to 2024-10-29T00:00:00Z, close 69947.6
- previous direction flat; decision close above entry high; new direction long
- entry channel window 2024-10-08T00:00:00Z inclusive to 2024-10-28T00:00:00Z exclusive
- entry high 69555 from daily starts 1729468800000
- entry low 58900 from daily starts 1728518400000
- exit channel at entry: high 69555, low 65240.2
- exit signal candle 2025-02-02T00:00:00Z to 2025-02-03T00:00:00Z, close 97645.7
- exit reason close below exit low
- exit channel window 2025-01-23T00:00:00Z inclusive to 2025-02-02T00:00:00Z exclusive
- exit high 107118.6 from daily starts 1737676800000
- exit low 97694.2 from daily starts 1737936000000
- entry fill 2024-10-29T00:01:00Z price 69939.9
- exit fill 2025-02-03T00:01:00Z price 97695.6
- price PnL = 0.001 * (97695.6 - 69939.9) = 27.7557
- fees 0; funding on 291 settlements summing to -3.3425634957559
- trade funding field -3.3425634957559; net = price - fees + funding = 24.4131365042441

### 2025 short trades

7 short round trips exit on or after 2025-01-01 and before 2026-01-01. A short that exits in 2024 is not in this list, even if its funding continued to affect the 2025 calendar split through the mark path of other positions.

### short -0.001 BTC, net -9.6109681555981 USDT

Exit fill is inside 2025.

- entry signal candle 2025-09-25T00:00:00Z to 2025-09-26T00:00:00Z, close 108931.3
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-09-05T00:00:00Z inclusive to 2025-09-25T00:00:00Z exclusive
- entry high 117884 from daily starts 1758153600000
- entry low 109888 from daily starts 1757116800000
- exit channel at entry: high 117884, low 110904.2
- exit signal candle 2025-10-01T00:00:00Z to 2025-10-02T00:00:00Z, close 118555.4
- exit reason close above exit high
- exit channel window 2025-09-21T00:00:00Z inclusive to 2025-10-01T00:00:00Z exclusive
- exit high 115779.6 from daily starts 1758412800000
- exit low 108550 from daily starts 1758844800000
- entry fill 2025-09-26T00:01:00Z price 108919.7
- exit fill 2025-10-02T00:01:00Z price 118662.5
- price PnL = -0.001 * (118662.5 - 108919.7) = -9.7428
- fees 0; funding on 18 settlements summing to 0.1318318444019
- trade funding field 0.1318318444019; net = price - fees + funding = -9.6109681555981

### short -0.001 BTC, net -9.0860222804859 USDT

Exit fill is inside 2025.

- entry signal candle 2025-04-06T00:00:00Z to 2025-04-07T00:00:00Z, close 78376.1
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-03-17T00:00:00Z inclusive to 2025-04-06T00:00:00Z exclusive
- entry high 88740 from daily starts 1742774400000
- entry low 81093.6 from daily starts 1742256000000
- exit channel at entry: high 88560.1, low 81146
- exit signal candle 2025-04-21T00:00:00Z to 2025-04-22T00:00:00Z, close 87471.1
- exit reason close above exit high
- exit channel window 2025-04-11T00:00:00Z inclusive to 2025-04-21T00:00:00Z exclusive
- exit high 86444 from daily starts 1744675200000
- exit low 78928.2 from daily starts 1744329600000
- entry fill 2025-04-07T00:01:00Z price 78300
- exit fill 2025-04-22T00:01:00Z price 87482.9
- price PnL = -0.001 * (87482.9 - 78300) = -9.1829
- fees 0; funding on 45 settlements summing to 0.0968777195141
- trade funding field 0.0968777195141; net = price - fees + funding = -9.0860222804859

### short -0.001 BTC, net -8.6872545762796 USDT

Exit fill is inside 2025.

- entry signal candle 2025-06-05T00:00:00Z to 2025-06-06T00:00:00Z, close 101455.2
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-05-16T00:00:00Z inclusive to 2025-06-05T00:00:00Z exclusive
- entry high 111968 from daily starts 1747872000000
- entry low 102000 from daily starts 1747612800000
- exit channel at entry: high 110700, low 103036.8
- exit signal candle 2025-06-09T00:00:00Z to 2025-06-10T00:00:00Z, close 110219.9
- exit reason close above exit high
- exit channel window 2025-05-30T00:00:00Z inclusive to 2025-06-09T00:00:00Z exclusive
- exit high 106773.7 from daily starts 1748908800000
- exit low 100350 from daily starts 1749081600000
- entry fill 2025-06-06T00:01:00Z price 101444.8
- exit fill 2025-06-10T00:01:00Z price 110157.5
- price PnL = -0.001 * (110157.5 - 101444.8) = -8.7127
- fees 0; funding on 12 settlements summing to 0.0254454237204
- trade funding field 0.0254454237204; net = price - fees + funding = -8.6872545762796

### short -0.001 BTC, net -6.5301439781904 USDT

Exit fill is inside 2025.

- entry signal candle 2025-08-02T00:00:00Z to 2025-08-03T00:00:00Z, close 112518
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-07-13T00:00:00Z inclusive to 2025-08-02T00:00:00Z exclusive
- entry high 123250 from daily starts 1752451200000
- entry low 112692.8 from daily starts 1754006400000
- exit channel at entry: high 120194.7, low 112692.8
- exit signal candle 2025-08-10T00:00:00Z to 2025-08-11T00:00:00Z, close 119271.8
- exit reason close above exit high
- exit channel window 2025-07-31T00:00:00Z inclusive to 2025-08-10T00:00:00Z exclusive
- exit high 118900 from daily starts 1753920000000
- exit low 111870 from daily starts 1754179200000
- entry fill 2025-08-03T00:01:00Z price 112509.6
- exit fill 2025-08-11T00:01:00Z price 119205.7
- price PnL = -0.001 * (119205.7 - 112509.6) = -6.6961
- fees 0; funding on 24 settlements summing to 0.1659560218096
- trade funding field 0.1659560218096; net = price - fees + funding = -6.5301439781904

### short -0.001 BTC, net -3.6109204357375 USDT

Exit fill is inside 2025.

- entry signal candle 2025-08-25T00:00:00Z to 2025-08-26T00:00:00Z, close 110069.7
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-08-05T00:00:00Z inclusive to 2025-08-25T00:00:00Z exclusive
- entry high 124571.2 from daily starts 1755129600000
- entry low 110551 from daily starts 1755993600000
- exit channel at entry: high 119208, low 110551
- exit signal candle 2025-09-10T00:00:00Z to 2025-09-11T00:00:00Z, close 113890
- exit reason close above exit high
- exit channel window 2025-08-31T00:00:00Z inclusive to 2025-09-10T00:00:00Z exclusive
- exit high 113369.4 from daily starts 1757030400000
- exit low 107202 from daily starts 1756684800000
- entry fill 2025-08-26T00:01:00Z price 109930.2
- exit fill 2025-09-11T00:01:00Z price 113919.7
- price PnL = -0.001 * (113919.7 - 109930.2) = -3.9895
- fees 0; funding on 48 settlements summing to 0.3785795642625
- trade funding field 0.3785795642625; net = price - fees + funding = -3.6109204357375

### short -0.001 BTC, net 1.549602612496 USDT

Exit fill is inside 2025.

- entry signal candle 2025-11-14T00:00:00Z to 2025-11-15T00:00:00Z, close 94533.3
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-10-25T00:00:00Z inclusive to 2025-11-14T00:00:00Z exclusive
- entry high 116373.3 from daily starts 1761523200000
- entry low 97931 from daily starts 1762992000000
- exit channel at entry: high 107481, low 97931
- exit signal candle 2025-12-03T00:00:00Z to 2025-12-04T00:00:00Z, close 93390.1
- exit reason close above exit high
- exit channel window 2025-11-23T00:00:00Z inclusive to 2025-12-03T00:00:00Z exclusive
- exit high 93081 from daily starts 1764288000000
- exit low 83755.3 from daily starts 1764547200000
- entry fill 2025-11-15T00:01:00Z price 94773
- exit fill 2025-12-04T00:01:00Z price 93334.7
- price PnL = -0.001 * (93334.7 - 94773) = 1.4383
- fees 0; funding on 57 settlements summing to 0.111302612496
- trade funding field 0.111302612496; net = price - fees + funding = 1.549602612496

### short -0.001 BTC, net 5.0588472991179 USDT

Exit fill is inside 2025.

- entry signal candle 2025-02-24T00:00:00Z to 2025-02-25T00:00:00Z, close 91478.2
- previous direction flat; decision close below entry low; new direction short
- entry channel window 2025-02-04T00:00:00Z inclusive to 2025-02-24T00:00:00Z exclusive
- entry high 101684.7 from daily starts 1738627200000
- entry low 93318.1 from daily starts 1739836800000
- exit channel at entry: high 99454.2, low 93318.1
- exit signal candle 2025-03-19T00:00:00Z to 2025-03-20T00:00:00Z, close 86792.3
- exit reason close above exit high
- exit channel window 2025-03-09T00:00:00Z inclusive to 2025-03-19T00:00:00Z exclusive
- exit high 86485 from daily starts 1741478400000
- exit low 76545 from daily starts 1741651200000
- entry fill 2025-02-25T00:01:00Z price 91730.7
- exit fill 2025-03-20T00:01:00Z price 86868.8
- price PnL = -0.001 * (86868.8 - 91730.7) = 4.8619
- fees 0; funding on 69 settlements summing to 0.1969472991179
- trade funding field 0.1969472991179; net = price - fees + funding = 5.0588472991179

Sum of these 2025-exit short nets: -30.9168595146776.

### All reconstructed zero-trading-cost trades

| Exit | Side | Price PnL | Funding | Net |
| --- | --- | ---: | ---: | ---: |
| 2023-02-10T00:01:00Z | long | -0.9665 | -0.1304045505118 | -1.0969045505118 |
| 2023-02-16T00:01:00Z | short | -2.72 | 0.0244750727753 | -2.6955249272247 |
| 2023-03-14T00:01:00Z | short | -2.4329 | 0.0248434469129 | -2.4080565530871 |
| 2023-04-22T00:01:00Z | long | 2.5482 | -0.2138168450688 | 2.3343831549312 |
| 2023-06-21T00:01:00Z | short | -2.551 | 0.0729447379995 | -2.4780552620005 |
| 2023-07-25T00:01:00Z | long | -0.8256 | -0.2187561026956 | -1.0443561026956 |
| 2023-08-30T00:01:00Z | short | -1.1095 | 0.0161059152616 | -1.0933940847384 |
| 2023-10-02T00:01:00Z | short | -2.8194 | 0.0430759138402 | -2.7763240861598 |
| 2024-01-23T00:01:00Z | long | 6.4702 | -1.7961443464525 | 4.6740556535475 |
| 2024-03-20T00:01:00Z | long | 17.6811 | -2.0945629956395 | 15.5865370043605 |
| 2024-05-16T00:01:00Z | short | -7.9248 | 0.1192109523042 | -7.8055890476958 |
| 2024-06-12T00:01:00Z | long | 0.4055 | -0.5065250607027 | -0.1010250607027 |
| 2024-07-15T00:01:00Z | short | 2.48 | 0.0335305847356 | 2.5135305847356 |
| 2024-08-03T00:01:00Z | long | -3.1983 | -0.2659241286155 | -3.4642241286155 |
| 2024-08-24T00:01:00Z | short | -5.8707 | 0.104133310004 | -5.766566689996 |
| 2024-09-14T00:01:00Z | short | -6.5784 | 0.0370557646945 | -6.5413442353055 |
| 2024-10-02T00:01:00Z | long | -0.8912 | -0.149482641965 | -1.040682641965 |
| 2025-02-03T00:01:00Z | long | 27.7557 | -3.3425634957559 | 24.4131365042441 |
| 2025-03-20T00:01:00Z | short | 4.8619 | 0.1969472991179 | 5.0588472991179 |
| 2025-04-22T00:01:00Z | short | -9.1829 | 0.0968777195141 | -9.0860222804859 |
| 2025-05-31T00:01:00Z | long | 10.4692 | -0.4260510691038 | 10.0431489308962 |
| 2025-06-10T00:01:00Z | short | -8.7127 | 0.0254454237204 | -8.6872545762796 |
| 2025-08-02T00:01:00Z | long | 2.0418 | -0.7592851785882 | 1.2825148214118 |
| 2025-08-11T00:01:00Z | short | -6.6961 | 0.1659560218096 | -6.5301439781904 |
| 2025-08-20T00:01:00Z | long | -10.426 | -0.1334373205656 | -10.5594373205656 |
| 2025-09-11T00:01:00Z | short | -3.9895 | 0.3785795642625 | -3.6109204357375 |
| 2025-10-02T00:01:00Z | short | -9.7428 | 0.1318318444019 | -9.6109681555981 |
| 2025-11-05T00:01:00Z | long | -18.8 | -0.5167411596323 | -19.3167411596323 |
| 2025-12-04T00:01:00Z | short | 1.4383 | 0.111302612496 | 1.549602612496 |

## Aggregates

| Book | Scenario | Net PnL | Gross price | Fees | Funding | Max drawdown | Position at end |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| DonchianDailyV1 | zero-trading-cost | -38.257778711447 | -29.2864 | 0 | -8.971378711447 | 67.0729369558912 | 0 |
| DonchianDailyV1 | Cost A | -41.402876785815 | -30.07267364 | 2.358824434368 | -8.971378711447 | 68.7710066120672 | 0 |
| Fixed long 0.001 BTC | Cost A | 48.360542973936 | 64.9174851 | 0.01360742094 | -16.543334705124 | 44.3887151092231 | 0.001 |

Calendar attribution is the change in sampled cumulative net PnL. A sample exactly at 1 January belongs to the year that is starting. 2023 starts on 21 January.

- zero-trading-cost 2023: -3.4431037454455 from 2023-01-21T00:00:00Z to 2024-01-01T00:00:00Z
- zero-trading-cost 2024: 11.0959146199872 from 2024-01-01T00:00:00Z to 2025-01-01T00:00:00Z
- zero-trading-cost 2025: -45.9105895859887 from 2025-01-01T00:00:00Z to 2026-01-01T00:00:00Z
- Cost A 2023: -3.7967613444135
- Cost A 2024: 10.2287410697712
- Cost A 2025: -47.8348565111727

Zero-trading-cost long trades 21.7104051047028; short trades -59.9681838161498.

## Cost A fixed long

Entry decision time is the performance start, 2023-01-21T00:00:00Z. Available at 2023-01-21T00:00:10Z. Fill at the stored one-minute open 2023-01-21T00:01:00Z, reference 22674.5, adjusted fill 22679.0349, fee 0.01360742094.

The position is still long 0.001 at the cutoff. Unrealized price PnL 64.9174851 is inside net PnL. No exit was invented.

Unadjusted-mark close estimate, abs(quantity) * terminal mark * (6+2) / 10000: 0.070077216.
Adjusted-close estimate, gap from the mark to an adverse sell plus a fee on that sell: 0.0700667044176.
The stored summary matches the adjusted-close estimate. The two estimates differ by 0.0000105115824 USDT. Neither is in net PnL.

## Discrepancies

Total recorded differences: 1. Differences other than reason labels and the disclosed close-cost formula: 0.

- `close_cost_definition` estimated_close_cost_usdt at `1767225600000`: expected `on unadjusted mark 0.070077216; on adjusted close fill 0.0700667044176`, recorded `0.0700667044176`. stored figure matches adjusted close fill. Difference between the formulas is -0.0000105115824. The figure is disclosed only and is not part of net PnL. Terminal mark and position are in the fixed_long_cost_a ledger.

## Conclusions

### 1. Decision correctness

The stored daily candles produce the recorded direction, target, channel bounds, and timestamps on every warm-up and evaluated candle, including no-change rows. The independent entry count is 29 and the independent exit count is 29.

### 2. Simulated execution and accounting

For the zero-trading-cost diagnostic and Cost A, independently selected one-minute opens, fees, adverse adjustments, funding cash flows, trade nets, calendar splits, and hourly-sampled drawdowns match the stored ledgers. The Cost A fixed-long comparator matches on the same basis, including its open terminal position. Execution-price adjustment is inside the fill price and is not subtracted again. The disclosed estimated closing cost is the one figure with two defensible formulas; it is not part of net PnL, and the difference is quantified above.

### 3. What remains unverified

The one-minute tape is sparse. Only the minutes required by these fills were checked, and only because those rows are present. Liquidity, partial fills, and live order behavior are not established. Fee and adverse-adjustment inputs are the prescribed research numbers. The higher-cost and delayed-execution scenarios were not reconstructed. There is no capital, margin, or liquidation model. This audit does not use the 2026 holdout.

## Screen result

DonchianDailyV1, daily 20/10, long/short, fixed 0.001 BTC: failed the 2023–2025 economic screen under the tested assumptions. Not approved for live deployment.

