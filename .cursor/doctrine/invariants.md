# Invariants

- No silent loss of private execution/order updates: lag must force reconcile or equivalent recovery.
- Submission journal: persist Submitted before send; reconcile by client order id on restart.
- Fill accounting must be idempotent under replays.
- Market-data sequence gaps drop the book rather than applying a bad merge.
- Stale books must not drive new exposure (staleness gate).
- Research crates must not enter the live dependency graph (`just test-5r-graph`).

<!-- doctrine-setup:invariants -->
