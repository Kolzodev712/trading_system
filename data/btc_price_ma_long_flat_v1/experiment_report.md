# BTCPriceMALongFlatV1 development experiment

Primary N=28 Cost A net: 20.6120701293059
Primary N=28 zero-cost net: 26.4553879050099
Fixed long Cost A net: 48.3605429739360 drawdown 44.3887151092231

Disposition: **reject**

- cost_a_net_positive: pass (net 20.6120701293059)
- cost_a_profit_retention_80pct: fail (net 20.6120701293059 requires >= 38.6884343791488)
- cost_a_drawdown_70pct_of_benchmark: pass (drawdown 30.6689062419564 requires <= 31.07210057645617)
- beats_notional_matched_passive: fail (net 20.6120701293059 vs scaled passive 26.349003953106737096013977584 (f=0.5448450809848760235550660848))
- higher_cost_net_positive: pass (net 11.8471204408099)
- delayed_execution_net_positive: pass (net 20.5469761215179)
- completed_round_trips_at_least_20: pass (completed trades 55)
- diagnostic_14_cost_a_positive: pass (net 20.7799676409438)
- diagnostic_70_cost_a_positive: pass (net 42.2272621972557)

Holdout 2026 was not opened. N=14/N=70 higher-cost and delayed ledgers are under backtest/diagnostic_n*/.
