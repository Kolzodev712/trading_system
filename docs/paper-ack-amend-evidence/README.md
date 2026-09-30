# Paper ACK / amend gate evidence

Isolated Paper on :19994. Previous two-level own-fix pack is at `docs/paper-own-fix-evidence/`.
Archive `/tmp/paper-levels-evidence-f02ca01` was not modified.

## Recorded SHAs at gate time

- k-paper HEAD: `5d3a0cc9391bd0bcb6e3c3090191fac279c53e9a`
- trading_system HEAD: `41cd257060a1dfdd84b090a44a0eb89ed3b782a0`
- RESULT: PASS (local lab). Paper / Demo / live remain **NO-GO**. Candidate #2 research remains separate.

`41cd257` is the last pushed HEAD. This PASS also used uncommitted adapter work: `AmendAck` now publishes the amended order into `ExchangeStateManager` so gRPC runner state matches Paper REST after 1010→1020.

## What to inspect

1. Delayed wrong ACK while B is pending
   - `catalog_after_wrong_ack.json`: A owns A; B has empty `owned_exchange_ids` and inflight client id only.
   - `runner_wrong_ack.txt` / `runner.log`: B place is in flight; delayed A ACK arrives with `client_order_id=None`, then with A's client id; neither attaches to B.
   - `catalog_after_b_ack.json`: B attaches only after its own correlated ACK.

2. Partial fill, additive replay, independent REST, restart
   - `lab_partial_fill_a.json` / `rest_after_partial.json`: A `PartiallyFilled` `filledAmount=0.001`.
   - `lab_partial_replay.json` / `rest_after_replay.json`: second 0.001 fill → `0.002`.
   - `rest_after_restart.json`: Paper REST still `PartiallyFilled` `0.002` after runner restart.
   - `catalog_after_restart.json`: A/B ownership not mixed.

3. Price-only amend at exact caps (`max_total_orders=1`, `total_max_position=0.01`)
   - `rest_before_amend.json`: 1010 / 0.010.
   - `rest_after_price_amend.json` and `grpc_status_after_amend.txt`: both 1020 / 0.010, same order id.
   - `amend_batch_counts.txt`: one amend batch; later ticks did not repeat.

## Residual (not a pass criterion of this pack)

Live runner position after two 0.001 fills showed `qty=0.004` while Paper REST filled amount stayed `0.002`. After restart, gRPC positions were empty because trading started disabled. Independent REST is the source of truth in this pack.
