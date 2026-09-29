# Progression

This is the home for increment status, open work, and next gates. Architecture (what the system is) lives in [ARCHITECTURE.md](ARCHITECTURE.md). The increment-by-increment record is [PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md](PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md). Other tracks: [TODO.md](../TODO.md), [proposals/](../proposals/).

Matching repo-local increment notes:

- k-paper: [PROP_TRADING_INTEGRATION_GUIDE.md](https://github.com/Kolzodev712/k-paper/blob/master/docs/PROP_TRADING_INTEGRATION_GUIDE.md)
- trading-system-ui: [PROP_TRADING_CONTROL_PLANE_GUIDE.md](https://github.com/Kolzodev712/trading-system-ui/blob/main/docs/PROP_TRADING_CONTROL_PLANE_GUIDE.md)

**Not claimed:** production complete, mainnet ready, or Candidate #2 as validated alpha.

## Accepted at stated scope

| Gate | Status | Evidence |
| --- | --- | --- |
| Increments 0–9A Paper qualification | Accepted locally | C2 → Paper, frozen N=28 adapter comparison, grpc and browser CancelWorking. Pins: `trading_system` `9dab54e`, k-paper `590f3ad` / `6d03acc`, UI `10496e7` (source review plus reported matrix; that review did not rerun the lab). |
| Increment 10 bounded Bybit Demo | Accepted (synthetic signal) | Pack `docs/inc10-evidence-measured-e923fa8/` (`042eac9`; runner `e923fa8`, UI `923dd30`, k-paper `6d03acc`). Independently flat before enablement; one 0.001 BTC buy and one 0.001 BTC reduce-only sell matching ledger and Demo history; ended flat with entries paused and flatten latch held. 90 ms is exchange-timestamp to local receive, not full order-execution latency. |
| Operator flatten durability (after Increment 10) | Implemented | Journaled reduce-only path; REST-verify cancel before send; `Acknowledged` settles to `Terminal` after a verified fill or flat venue. Paper two-flatten and cancel-fail recovery: `c29b9e5`. |

No further Demo run is required to keep Increment 10 accepted. Do not Resume Bybit Demo, reuse the Hold fixture, or start Increment 11 unless the owner asks.

## Open

1. Validate Candidate #2 as a strategy. Demo tested execution with a synthetic signal. Operational readiness is not alpha.
2. Separate mainnet readiness and approval. Do not document the system as production complete or mainnet ready.

## Next (not started)

**Increment 11** — research-framework cleanup for Candidate #3. Do not start until the owner asks.

Python DSL / DEX / HFT proposals are separate tracks (`proposals/`). They are not this qualification.

## Cursor

Increment 10 is accepted. Architecture docs describe the implemented system only. This file owns progression. Do not Resume. Do not rerun Demo for the Increment 10 gate. Candidate #2 remains an operational baseline, not validated alpha.
