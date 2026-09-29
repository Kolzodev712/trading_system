# Long-running Bybit Demo runbook (real signal)

This is the **unattended-capable** Candidate #2 path on Bybit Demo: real completed daily candles, `execution_mode=exchange`, no synthetic clock. It is **not** Increment 10’s five-minute measured launcher.

Do **not** start this with `scripts/inc10_bybit_demo.sh`. That script was built to measure for five minutes and then flatten.

Candidate #2 remains an **operational baseline**, not validated alpha. A Demo run shows how the real signal behaves through the Demo execution path. It does not change that verdict.

**Do not Resume, enable, or place Demo orders from this document until the owner authorizes a live run.** The procedure below is what that run must follow.

## What this uses

- Composition binary: `btc_price_ma_app --mode strategies`
- Config: `sample_config/bybit-demo/` — Bybit, `demo: true`, `testnet: false`, Futures, BTC/USDT only, N=28, `max_entry_age_secs=86400`
- Bounded cap: `0.001` BTC, one working order, `max_notional` 150
- Persistence under `runtime-data/bybit-demo/` (gitignored): journal, ledger, runtime snapshot, restart flag, logs, operator token
- Operator: `TRADING_OPERATOR_TOKEN` + `TRADING_OPERATOR_ID` (Envoy injects the same token for the UI)
- MongoDB must be up (`mongo.enabled` is true)

`sample_config/bybit/bot.json` is a placeholder Spot ETH/SOL config. It is unsuitable.

## Forbidden env

Leave unset for the entire process (including child shells):

- `TRADING_TEST_CANDLES_PATH`
- `TRADING_TEST_NOW_MS`
- `INC10_FRESH_ENTRY_FIXTURE`

The last flat-to-long on real dailies may be stale. **Expect Hold.** Do not widen `max_entry_age_secs` and do not inject candles to force an entry.

## 1. Persistence and operator identity

```bash
mkdir -p runtime-data/bybit-demo/logs
cp sample_config/bybit-demo/operator.env.example runtime-data/bybit-demo/operator.env
# Put a Demo API key/secret into a local copy of bot.json (not the committed placeholders).
# Recommended: copy sample_config/bybit-demo/bot.json → runtime-data/bybit-demo/bot.json
# and edit credentials there.
python3 -c 'import secrets; print(secrets.token_urlsafe(24))' \
  > runtime-data/bybit-demo/operator.token
chmod 0600 runtime-data/bybit-demo/operator.token
```

Fill `TRADING_OPERATOR_TOKEN` from that file (do not print it). Example:

```bash
set -a
source runtime-data/bybit-demo/operator.env
set +a
export TRADING_OPERATOR_TOKEN="$(head -n1 runtime-data/bybit-demo/operator.token)"
unset TRADING_TEST_CANDLES_PATH TRADING_TEST_NOW_MS INC10_FRESH_ENTRY_FIXTURE
```

If the UI Envoy is in front of gRPC, point `TRADING_OPERATOR_TOKEN_FILE` at `runtime-data/bybit-demo/operator.token` (uid that Envoy’s worker can read; do not chmod 0644).

Journal, ledger, and snapshot paths in the Demo `bot.json` already point at `runtime-data/bybit-demo/` so they survive process restart. Do not delete them between days.

## 2. MongoDB

Strategies mode requires MongoDB.

```bash
docker compose up -d mongodb
```

`docker-compose.yml` publishes Mongo on **host 27018**. If you use that container, set `mongo.connection_string` to `mongodb://localhost:27018`. A mongod already on `27017` can keep the sample default.

Confirm the ping succeeds before launch.

## 3. Startup (paused)

```bash
BOT=runtime-data/bybit-demo/bot.json   # or the edited sample path
STRAT=sample_config/bybit-demo/strategies.json

cargo build -p btc_price_ma_app -p cli --bin grpc_ctl

mkdir -p runtime-data/bybit-demo/logs
cargo run -p btc_price_ma_app -- --mode strategies \
  --config "$BOT" \
  --strategies-config "$STRAT" \
  2>&1 | tee -a runtime-data/bybit-demo/logs/runner.log
```

Non-Paper Exchange starts with **entries paused**. Global trading stays off until you enable. `initially_enabled=true` lets the model reconstruct; it must not send while paused.

If a previous snapshot has `global_trading: true`, the process will restore that. For a first start-gate, start with no snapshot file (or set `global_trading` false) so nothing is live until you Resume.

## 4. Start gate (before any enable / Resume)

In another shell, with the same operator env:

```bash
export TRADING_STRATEGY=BTCPriceMALongFlatV1
export TRADING_SYMBOL=BTC/USDT
STATUS="$(cargo run -q -p cli --bin grpc_ctl -- status)"
echo "$STATUS"
```

Confirm all of:

| Check | How |
| --- | --- |
| Venue / env / mode | `venue=BYBIT`, `environment=DEMO`, `execution_mode=exchange` |
| Operator identity | `operator_id=` matches `TRADING_OPERATOR_ID` |
| Paused, no flatten latch unless leftover | `entries_paused=true`; flatten latch only if a prior flatten is still in progress |
| Cap | strategies file `max_position` / `max_order_qty` `0.001`, `max_total_orders` 1 |
| No order while paused | runner log has no `placing order` / Demo REST place |
| Healthy public book | live BTC/USDT orderbook updates; `max_market_data_age_ms=15000` |
| Private stream | Demo private WS connected (`wss://stream-demo.bybit.com/v5/private`); Demo has no WS Trade — orders are REST |
| Completed-candle reconstruction | N=28 close window on **completed** dailies; forming bar dropped |
| Clean REST reconciliation | no lasting trading-level discrepancy; open-order REST succeeds |
| Runner vs Demo REST | `python3 scripts/bybit_demo_rest_snapshot.py "$BOT" --grpc-status "$STATUS"` |

Unauthorized mutating RPC (no token) must be denied.

Hold on a stale last flat-to-long is **success** at this gate, not a reason to inject a fixture.

Only after the table passes: enable the strategy, then `Resume` for the authorized bounded run.

```bash
cargo run -q -p cli --bin grpc_ctl -- enable BTCPriceMALongFlatV1
cargo run -q -p cli --bin grpc_ctl -- resume
```

## 5. Monitoring

- `grpc_ctl status` and the UI: venue DEMO, pause/flatten flags, working orders, position — **RPC success is not flat**.
- Tail `runtime-data/bybit-demo/logs/runner.log`.
- Append-only `submission-journal.jsonl` / `execution-ledger.jsonl`. Operator flatten rows must move `acknowledged` → `terminal` on a verified fill or while the account is flat (private stream + startup/recon settle). Do not wait for the next FlattenAndPause. Bybit Demo retains order history about **seven days**; an old acknowledged exit that is still unsettled after history expires can block a later flatten.
- Periodic independent REST: `scripts/bybit_demo_rest_snapshot.py`.
- Expect Hold until a **fresh** real daily transition; let the candles drive it.

## 6. Shutdown

Pause and cancel first. Flatten only if a position or working reduce-only remains. Then verify **both** Demo REST and the runner show zero working orders and a flat position before killing the process.

```bash
cargo run -q -p cli --bin grpc_ctl -- pause
cargo run -q -p cli --bin grpc_ctl -- cancel
# flatten only if still open:
cargo run -q -p cli --bin grpc_ctl -- flatten
STATUS="$(cargo run -q -p cli --bin grpc_ctl -- status)"
python3 scripts/bybit_demo_rest_snapshot.py "$BOT" --grpc-status "$STATUS" --require-flat
```

Then stop the runner (SIGINT / UI restart with `cancel_working` or `flatten` as intended). Leave `runtime-data/bybit-demo/` in place.

## 7. What this run is for

How Candidate #2’s **real** daily signal behaves on the Demo execution path (Hold vs TargetPosition, book freshness, recon, operator flatten settlement). It does **not** promote Increment 11, mainnet, or alpha.
