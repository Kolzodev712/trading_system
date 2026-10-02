# Trading Report V1

## Intent

A person can run one read-only command and copy a five-item JSON/Markdown brief into a conversation. The brief says where/when, what the market is doing, what venue exposure exists, what the system is trying and allowed to do, and whether that snapshot can be trusted. It does not recommend a trade or move money.

## Constraints

- Observer only. Runner and trading engine keep economic authority.
- `trading_report` is never imported by `runner`, `trading_engine`, `gateway`, `strategy_core`, `research_engine`, `research_cli`, or `btc_price_ma_app`.
- Failed gRPC, missing monitoring payload, incomplete REST pagination, or absent Levels stay unavailable. They never become an empty account or a successful verdict.
- Journal latest-state is a read-only projection. A missing file is not a known-empty journal.
- Flatten/RPC success is not flat. Venue REST is the independent account check.
- No AI calls, daemon, UI viewer, research-run ingestion, k-paper change, or Demo mutation.

## Design

New workspace crate `src/trading_report` with `trading-report collect`. It snapshots each configured runner over typed gRPC (`GetTradingStatus`, `GetMonitoringData`, `GetLevels` in Levels mode, `GetRiskSnapshot`) and independently walks Paper or Bybit Demo REST.

Narrow production change: monitoring stops age-filtering working orders; monitoring records source availability instead of collapsing errors to `[]`; gateway exposes a live `GetRiskSnapshot` from the same `RiskManager` the runner uses.

Verdicts are `verified` | `stale` | `unverified` | `disagreement`. `actionable_for_analysis` is true only for `verified`. Two subjects never share a risk allocator.

## Tests

- Unit: schema, missing/default fields, freshness, decimal/symbol compare, pagination, Spot vs Futures truth, journal latest-state, verdict aggregation.
- Process: Levels Exchange plus Strategies Shadow; REST-vs-runner set compare; saved combined overall verdict; missing GetTradingStatus; Spot inventory vs trading-state qty; freshness at collection completion; venue stamp is the earliest REST response (a later delayed call does not reset age to 0); unresolved journal; malformed REST; secret rejection before write.
- Bybit Demo signed REST is `NOT_RUN` without credentials. No Resume or orders.
