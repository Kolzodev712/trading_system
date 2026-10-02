# BTCLiquidationShockV1

Disposition: reject

Revision: 8e20b5f53b83101c2dba05e823d9ad69d91ccd9e

Cost A is the synthetic 6 bps + 2 bps + 10 s research case, not a Bybit fee quote.

Reversal Cost A net -57.3483451072483 (annualized -19.466991682548737523234200743 over the window), trades 416, drawdown 69.8788584640137, turnover 54223.37330582, time in market 8985600000 ms, average absolute notional 6.2991469289448209099709583737.

Long-shock net -32.8969526705106 / short-shock net -24.4513924367377 under Cost A. Mean trade -0.1378565988155007211538461538 / median -0.04761436630750.

Placebo Cost A net -29.4090476927517. Exposure-scaled passive net 5.3603560048462392001251952068. Candidate #2 primary Cost A net 20.6120701293059 is context only.

Entry year 2023: -3.0370205363058
Entry year 2024: -25.2664317449723
Entry year 2025: -29.0448928259702

- completed_trades_at_least_20: pass (completed trades 416)
- cost_a_net_positive: fail (net -57.3483451072483)
- higher_cost_net_positive: fail (net -122.4164020528563)
- delayed_net_positive: fail (net -58.2887238598203)
- cost_a_beats_placebo: fail (reversal -57.3483451072483 vs continuation -29.4090476927517)
- not_one_trade: fail (net -57.3483451072483 after removing winner 4.226758372816 is -61.5751034800643)
- not_two_negative_entry_years: fail (3 of 2023/2024/2025 are negative)

primary promotion failed; the robustness neighborhood was not opened

Trailing-sigma terciles (descriptive): low -17.7141954249467 medium -29.0745958524754 high -10.5595538298262.
