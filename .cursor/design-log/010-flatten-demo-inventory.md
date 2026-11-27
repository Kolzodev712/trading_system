# Flatten Demo Inventory

## Problem / context

Bybit demo still holds ~0.048 BTC USDT linear from controlled BuyLowSellHigh fires. TSM reconciles that as current qty. Design Log #009 sets initial `max_position` to 0.001. Inheriting 0.048 means the book is already 48× over cap: the next strategy can only reduce or Hold, and a flatten-to-zero from 0.048 is a large live-shaped close if we ever pointed at mainnet with the same inventory.

## Decision

**Flatten to flat (qty 0) on demo before the first prop strategy and before enabling trading under the 0.001 cap.** Do not inherit the leftover as starting exposure.

How: one controlled reduce-only close (or `TargetPosition` 0) on demo, then confirm ESM/TSM qty is 0. Keep trading off afterward.

## Alternatives

- **Inherit 0.048** — Rejected for initial phases: over cap, and “lose as little as possible” means no leftover risk.
- **Partial reduce to 0.001** — Rejected: leftover is test junk, not a chosen live book.

## Consequences

- Flattening 0.048 on **demo** realizes demo P&L only; do not run this flatten against mainnet keys.
- After flatten, a 0.001 cap is a clean start. Any later live fire starts from zero.

## Verification

- REST/WS position for BTC/USDT linear is 0 (or below 1e-6).
- TSM reconcile: internal = exchange = 0.
- Trading remains disabled until the first prop strategy is ready.

## Implementation results

Shipped: `TRADING_FLATTEN=1` on strategies-mode start fetches REST positions and market-closes each `|qty| > 1e-6` with `reduceOnly=true` (Bybit maps `exchange_params["reduceOnly"]`). Demo only.

Verified: first run sold leftover `0.048` BTC/USDT (`Sell 0.048` filled @ 76557.1). Restart: REST `0` futures positions, `Flatten: no open futures positions`, TSM reconcile no discrepancies, `grpc_ctl status` `positions=0` / `pos=0`, trading off.

Caveat: immediately after the first close, periodic reconcile briefly saw `internal=0, exchange=0.048` (stale ESM snapshot) and wrote qty back. Restart against REST cleared it. Do not flatten on mainnet. Follow-up: ignore stale ESM qty right after a flatten fill.

