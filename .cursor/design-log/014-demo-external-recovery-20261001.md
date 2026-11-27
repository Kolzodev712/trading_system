# Demo External Recovery 20261001

## Problem / context

After Demo Levels `demo-mutating-20261001T130502Z` at SHA `fef18bb`, FlattenAndPause left a Bybit Demo BTCUSDT linear one-way long of 0.001 with runner 274988 latched. Repeating FlattenAndPause could not close it. The leftover had to be closed outside the runner without Resume or a second harness mutate.

## Decision

Keep runner 274988 latched. Take fresh signed Demo REST snapshots. When they matched the expected one-way long ≤ 0.001 with no working orders, submit one reduce-only Market Sell for the verified size (`positionIdx=0`, unique `orderLinkId`). Treat ACK as insufficient: lookup that exact link id on realtime then history, then independently REST-verify position and working orders. Stop the runner only after REST position=0 and working=0. Record this recovery separately from the FlattenAndPause failure (Design Log #013). Demo Levels gate remains FAIL.

## Alternatives

- Repeat grpc `FlattenAndPause` — rejected: latch already held and no reduce-only was being sent.
- Resume or rerun the Demo harness — rejected: would add exposure on a non-flat account.
- Kill the runner before REST-flat — rejected: process must stay up until venue flat is proven.

## Consequences

Account is independently REST-flat via an **external** close, not via FlattenAndPause. That does not qualify Demo Levels, Candidate #2, or real-funds trading. Operator token from the mutate was discarded; recovery used bot Demo keys against `api-demo.bybit.com` only.

## Verification

- Before: REST working=0, position Buy 0.001, `positionIdx=0`, leverage 1x.
- Close `orderLinkId=7515a72b-4af6-4a4c-9aa7-faeb773dce31` / `orderId=f7f17bc7-4ed5-4c21-ab52-236adb5c3c5b` status Filled cumExecQty=0.001.
- After/final: REST positions=[], working=[].
- Runner 274988 stopped only after that proof.

## Implementation results

Artifacts: `artifacts/workflows/demo-mutating-20261001T130502Z/external-recovery/` (before/after REST JSON, close ACK/lookup/IDs, runner.log). Gate `results.json` is unchanged FAIL.
