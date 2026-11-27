# Hybrid Strategy Config

## Problem / context

Strategy params live in `strategies.json` and are load-once. Changing `max_position` or `strategy_type` means editing a file and bouncing the process by hand. The UI can only toggle trading / enable a strategy. Rebuilds for operator knobs are the pain; secrets must not move into the UI. See Design Logs #008–#011.

## Decision

Three layers. One bot. Exchange remains position truth.

```text
Deploy (not UI)          Live gRPC (no restart)         Persist + restart
keys, hosts, mongo       trading on/off                 strategy_type
venue, pairs, demo       strategy enable                thresholds / size
leverage, bind           max_position cap               history windows
```

**Deploy.** Stay in gitignored `config/cex/bot.json` (or env). Not shown, not written by the UI. Not compiled into the binary.

**Live.** Keep `SetTradingEnabled` and `ToggleStrategy`. Add `UpdateStrategyCap` that mutates `StrategyExecutor.strategy_caps` under the existing lock. Next `evaluate` uses the new cap. Cap-down of an oversized book can only flatten or Hold (same as #009).

**Persist + restart.** Operator file remains `config/cex/strategies.json` (or a sibling `operator.json` if we later split BLSH-shaped leftovers). UI/gRPC `UpdateStrategyConfig` writes that file. Fields that require a new strategy instance (`strategy_type`, thresholds, `position_size`, history) apply only after a controlled restart.

**Snapshot (not a book dump).** On restart request, write a small `runtime-snapshot.json`:

- `mode` (strategies)
- `strategy_id`s and `enabled`
- `global_trading`
- `on_stop`: `cancel_working` (today’s shutdown) | `flatten` | `keep_working`

On start: load strategies file → bootstrap from exchange (ESM/TSM) → restore enable / trading from snapshot. Do **not** persist positions, balances, or SMA history. PropHold does not need warmup; BLSH SMA resets.

**Restart sequence**

1. Refuse if deploy fields would change (keys, pairs, demo).
2. Set trading off.
3. Apply `on_stop`.
4. Write snapshot + operator file.
5. Exit runner (existing shutdown timeout).
6. Host script starts the same binary/mode (no `cargo build`).
7. Restore flags from snapshot.

v1 script can be `scripts/restart-strategies.sh` plus `grpc_ctl`. UI button is a later `RequestRestart` on the same gRPC (same trust as enable-trading). No Mongo strategy store in v1 — one bot, one file.

## Alternatives

- **Everything live** — Rejected: swapping `strategy_type` mid-process is a new instance; leverage/demo are venue identity.
- **Mongo as source of truth** — Deferred: levels already do this; strategies do not need multi-writer yet.
- **Position snapshot file** — Rejected: exchange already resumes the book; a local dump can be stale and dangerous.

## Consequences

- UI still never sees API keys.
- A cap change is immediate; a type/threshold change costs one clean bounce.
- After restart, SMA/price history is empty (Hold until warmup). Enable flags come back; they do not today.
- Restart RPC/script can halt the bot if the host supervisor is missing — script must be the process manager (systemd/just), not “hope the old PID respawns.”
- `live_config` crate stays levels-only. Do not dual-path strategies through it.

## Verification

- Live cap: gRPC `UpdateStrategyCap` → next intent clipped; no restart; unit test on the cap map.
- Restart-required field: file changes, running instance unchanged until bounce.
- Restart script: trading off → stop → start → `grpc_ctl status` shows same `strategy_id`, restored `enabled`, `positions` match REST (not the snapshot).
- Snapshot never contains qty/keys.
- UI cannot read or write `bot.json` credentials.

## Change list (v1)

**Proto (both repos must match)**
- `libs/trading_core/proto/trading_cli.proto` and UI `proto/trading_cli.proto`
- RPCs: `GetStrategyConfig`, `UpdateStrategyCap`, `UpdateStrategyConfig`, `RequestRestart`
- `StrategyStatus.max_position` (streamed; not secrets)

**trading_system**
- `StrategyExecutor.strategy_caps` → shared `Arc<RwLock<HashMap>>` so gRPC can mutate
- Gateway: persist `strategies.json`; apply cap live; write `config/cex/runtime-snapshot.json` + `restart.requested`; signal existing `ShutdownManager`
- Runner start: restore enable + `global_trading` from snapshot (exchange still owns qty)
- `scripts/run-strategies.sh`: relaunch while `restart.requested` exists (no `cargo` rebuild)
- `grpc_ctl`: `cap`, `restart`
- Do not expose `bot.json` credentials on any RPC

**trading-system-ui**
- Copy proto, `npm run generate`
- Strategy details: load operator config, edit cap (live), edit other params (persist; toast if restart needed), Restart button
- API wrappers in `tradingCliService.ts`
- No credential fields

## Implementation results

Shipped as designed. No credentials on any new RPC. Snapshot is flags only.

**trading_system**
- Shared `Arc<RwLock<HashMap>>` caps; `UpdateStrategyCap` mutates the map and `strategies.json`
- `GetStrategyConfig` / `UpdateStrategyConfig` persist operator knobs; `needs_restart` when non-cap fields change
- `RequestRestart` writes `runtime-snapshot.json` + `restart.requested`, disables trading, signals `ShutdownManager`. v1 `on_stop` is `cancel_working` only
- Strategies start restores enable + `global_trading` from snapshot after exchange bootstrap
- `scripts/run-strategies.sh` relaunches `target/release/runner` when the flag exists (no rebuild)
- `grpc_ctl cap|config|restart`

**trading-system-ui**
- Proto copied + `npm run generate`
- Strategy details: live cap, persist other knobs, Restart button
- Strategies table shows cap
- `docs/grpc-api.md` lists the new RPCs

**k-paper:** no proto or UI copy to change.

**Verification**
- `cargo check` runner / gateway / cli / trading_engine
- `cargo clippy -p runner --no-deps --all-targets -- -D warnings` and same for `trading_engine`; `grpc_ctl` clippy clean. Gateway `-D warnings` still fails on pre-existing `result_large_err`
- Tests: snapshot roundtrip (no qty/keys), per-strategy caps, three strategy monitoring integration tests
- UI: `tsc -b`, `npm test` (20 passed)
- Live UI: operator changed a value, Restart reconnected the dashboard, runner came back with the new value loaded

**Deviations**
- `flatten` / `keep_working` rejected until implemented
- Changing `strategy_type` is not a live swap; it is the lookup key
