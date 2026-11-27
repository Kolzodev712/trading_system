# Flatten And Pause Stale Entry

## Impact

Bybit Demo Levels gate `demo-mutating-20261001T130502Z` at SHA `fef18bb` failed on cleanup after a real fill. FlattenAndPause latched pause/flatten but never journaled a reduce-only exit. Venue leftover was BTCUSDT linear Buy 0.001. Demo Levels remains FAIL; this does not qualify Candidate #2.

## Detection / repro

Authorized bounded Demo mutate: one Resume, two entry creates, disable+REST-cancel of create 1 without CancelWorking, create 2 filled 0.001. Protective FlattenAndPause retries all returned `venue position unverified: REST positions still open (BTC/USDT:0.001)`. Runner log: `HOLD_FLATTEN_LATCHED`; CancelWorking kept reporting one TSM order while independent REST working=0. Journal had only the two Buy rows; ledger had only the buy fill.

## Root cause

`submit_operator_reduce_only_exits` skipped REST-open targets when `has_working_exit` found **any** TSM working order on the pair. After the REST cancel of create 1, TSM still cached that Buy as `New`. REST working-book verify passed (empty), so flatten proceeded, then silently retained zero targets and failed the later REST position proof. A stale cached **entry** is not an inflight reduce-only exit.

## Fix

Exit suppression requires a **correlated, unresolved journaled `operator_flatten` row** and **verified venue order state** (paginated REST working book plus `orderLinkId` open/history lookup). A REST position target is never dropped silently.

- Verified live journaled exit on the venue → wait (do not send a duplicate).
- Journaled exit terminal on venue while REST position remains → send a new journaled reduce-only.
- Venue lookup failure or inconclusive open/history → explicit unverified, fail closed, do not send.
- No journaled operator exit → send. Stale TSM Buy or Sell with REST working `[]` is a reconciliation mismatch, not a suppressor.

CancelWorking / FlattenAndPause log TSM working IDs absent from REST after cancel so the stale cache stays visible.

## Caveats / follow-ups

- Demo Levels gate for run `demo-mutating-20261001T130502Z` stays FAIL.
- External recovery of that leftover is Design Log #014; it is not a FlattenAndPause proof.
- Opposite-side TSM cache alone must not suppress. A next bounded Demo run (not authorized here) is what tests FlattenAndPause on venue.

## Verification

- Unit: `stale_cached_sell_with_rest_working_empty_sends` sends; `genuine_live_operator_exit_waits` waits; `rest_lookup_failure_fails_closed` and `rest_working_book_failure_with_inflight_fails_closed` fail closed.
- Paper: `paper_flatten_after_stale_entry` still required before another Demo qualification.

## Implementation results

Follow-up: `has_working_exit` TSM side-check is no longer used to drop REST targets. `submit_operator_reduce_only_exits` correlates journal + paginated REST working orders + `orderLinkId` lookup. Demo mutate not rerun. Gate `demo-mutating-20261001T130502Z` remains FAIL.
