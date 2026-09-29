# Increment 10 measured Demo evidence (sanitized)

Pinned runner `e923fa8`, plan record `edb097e`, UI `923dd30`. Source workdir `/tmp/inc10-bybit-demo-measured-e923fa8` on the lab host. No rerun. No mainnet. Candidate #2 is not validated alpha.

## Included

- `submission-journal.jsonl`
- `execution-ledger.jsonl`
- `grpc_status_paused.txt`
- `grpc_status_enabled.txt`
- `grpc_status_after_measure.txt`
- `grpc_status_end.txt`
- `venue_before_enable.json`
- `venue_end_flat.json`
- `runner-restart.log` (API key, secret, and operator token redacted)
- `demo_order_history.json` — read-only Bybit Demo `GET /v5/order/history` and `GET /v5/execution/list` for 2026-09-29T12:21:40Z–12:27:20Z

Archive: `docs/inc10-evidence-measured-e923fa8.tar.gz` (sibling of this directory). Per-file hashes: `SHA256SUMS`.

## Excluded

`bot.json`, API credentials, `operator.token`.

## How to read the four “placing order via REST” lines

`BybitRpcClient::send_via_rest` logs that string **before** matching `CreateOrder` / `CancelOrder` / `CancelAllOrders`. CancelWorking and FlattenAndPause each call `cancel_all_orders`. Those log lines do not count sell creates.

Demo history for the window: **two** user creates, both filled.

| orderId | orderLinkId | side | type | reduceOnly | qty |
| --- | --- | --- | --- | --- | --- |
| `4fd15467-3060-4ea2-9045-681539918fcd` | `bd235f19-a599-464c-8494-a59d52e69450` | Buy | Limit | false | 0.001 |
| `64eb5576-ab5f-4e5e-bc87-ad0136c84870` | *(empty)* | Sell | Market | true | 0.001 |

The buy’s `orderLinkId` is the journal client ID. Distinct exchange `orderId` vs client ID is by design.

## Production follow-ups (no further Demo mutation for this review)

1. FlattenAndPause’s reduce-only exit is sent via `trading_client().open_order` and is **not** appended to the submission journal. History shows empty `orderLinkId` on the sell. Ledger can still record the fill; restart recovery by persisted client ID does not cover that exit.
2. `flatten_and_pause` does `let _ = self.trading_control.cancel_working(...)`. A CancelWorking error inside FlattenAndPause is ignored.

Neither item proves this run ended incorrectly. Independent REST+runner were flat at stop.

Later `trading_system` work journals operator flatten client IDs and reports CancelWorking from FlattenAndPause. That does not change this evidence pack.
