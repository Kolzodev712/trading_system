#!/usr/bin/env bash
# Remaining Paper levels paths: Spot sell from inventory, UI disable/delete,
# Futures lifecycle. Reuses ACK/amend and own-fix packs for already-proven paths.
# Isolated Paper. Does not touch /tmp/paper-levels-evidence-f02ca01 or :19996.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KPAPER_DIR="${K_PAPER_DIR:-/home/kolzo-dev/Desktop/k-paper}"
WORK="${PAPER_SIGNOFF_WORKDIR:-/tmp/paper-signoff}"
EV="$WORK/evidence"
PORT="${PAPER_EXCHANGE_PORT:-19994}"
HTTP="http://127.0.0.1:${PORT}"
HEALTH="http://127.0.0.1:18086/health"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
RUNNER="$TARGET_DIR/debug/runner"
GRPC="$TARGET_DIR/debug/grpc_ctl"
KPAPER_TARGET="${CARGO_TARGET_DIR:-$KPAPER_DIR/target}"
KPAPER_BIN="$KPAPER_TARGET/debug/k-paper"
PACK="$ROOT/docs/paper-signoff-evidence"

export TRACE_JAEGER_DISABLED="${TRACE_JAEGER_DISABLED:-1}"
export TRADING_GRPC="${TRADING_GRPC:-http://127.0.0.1:50051}"

mkdir -p "$WORK" "$EV"
rm -f "$WORK"/runner*.log "$WORK"/k-paper.log "$WORK"/*.jsonl "$WORK"/runtime-snapshot.json "$WORK"/levels.json

fail() { echo "FAILED: $*" >&2; exit 1; }

wait_http() {
  local url=$1
  local n=${2:-40}
  for _ in $(seq 1 "$n"); do
    if curl -sfS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  fail "timeout waiting for $url"
}

paper_get() { curl -sfS "$HTTP/$1"; }
paper_post() {
  local path=$1
  local body=${2:-'{}'}
  curl -sfS -X POST "$HTTP/$path" -H 'content-type: application/json' -d "$body"
}

kill_pidfile() {
  local f=$1
  [[ -f "$f" ]] || return 0
  local pid
  pid="$(cat "$f")"
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 40); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.1
    done
    kill -KILL "$pid" 2>/dev/null || true
  fi
  rm -f "$f"
}

cleanup() {
  if [[ "${PAPER_SIGNOFF_KEEP:-}" == "1" ]]; then
    echo "KEEP=1 leaving Paper pid=$(cat "$WORK/k-paper.pid" 2>/dev/null || true) runner pid=$(cat "$WORK/runner.pid" 2>/dev/null || true)"
    return 0
  fi
  kill_pidfile "$WORK/runner.pid"
  kill_pidfile "$WORK/k-paper.pid"
}
trap cleanup EXIT

start_runner() {
  local log=$1
  "$RUNNER" --mode levels --config "$WORK/bot.json" --levels-config "$WORK/levels.json" \
    >"$log" 2>&1 &
  echo $! >"$WORK/runner.pid"
  wait_http "$HEALTH" 80
  for _ in $(seq 1 40); do
    if curl -sfS "$HEALTH" | grep -q 'cex_ready=true'; then
      return 0
    fi
    sleep 0.25
  done
  tail -n 80 "$log" || true
  fail "runner health not cex_ready ($log)"
}

start_kpaper() {
  local cfg=$1
  PAPER_EXCHANGE_PORT="${PORT}" \
  PAPER_EXCHANGE_CONFIG="$cfg" \
  PAPER_EXCHANGE_PROFILE=lab \
  "$KPAPER_BIN" >"$WORK/k-paper.log" 2>&1 &
  echo $! >"$WORK/k-paper.pid"
  wait_http "$HTTP/open_orders"
}

wait_open_at() {
  local price=$1
  local out=$2
  local id=""
  local i
  for i in $(seq 1 40); do
    paper_get open_orders >"$out"
    id=$(python3 - "$price" "$out" <<'PY'
import json,sys
price,path=sys.argv[1],sys.argv[2]
orders=json.load(open(path))
for o in orders:
    if str(o.get("price","")).startswith(price):
        print(o["orderId"]); break
PY
)
    if [[ -n "$id" ]]; then
      echo "$id"
      return 0
    fi
    sleep 0.25
  done
  fail "no working order at $price"
}

assert_spot_qty() {
  local expect=$1
  local tag=$2
  local ok=0
  local i
  for i in $(seq 1 40); do
    paper_get balances >"$EV/balances_${tag}.json" || true
    "$GRPC" status >"$EV/grpc_status_${tag}.txt" 2>/dev/null || true
    if python3 - "$expect" "$tag" <<'PY'
import json,sys,re
from pathlib import Path
expect=float(sys.argv[1])
tag=sys.argv[2]
ev=Path("/tmp/paper-signoff/evidence")
eps=1e-6
paper=0.0
bp=ev/f"balances_{tag}.json"
if bp.exists() and bp.stat().st_size>0:
    raw=json.loads(bp.read_text() or "[]")
    rows=raw if isinstance(raw,list) else raw.get("balances",[])
    for b in rows:
        if str(b.get("asset") or b.get("symbol") or "").upper() in ("BTC","XBT"):
            paper=float(b.get("total") or b.get("available") or 0)
            break
status=(ev/f"grpc_status_{tag}.txt").read_text() if (ev/f"grpc_status_{tag}.txt").exists() else ""
m=re.search(r"position BTCUSDT qty=([0-9.]+)", status)
runner=float(m.group(1)) if m else 0.0
print(f"qty-agree {tag}", {"paper_btc":paper,"runner":runner}, "expect", expect)
if abs(paper-expect)>eps or abs(runner-expect)>eps:
    sys.exit(2)
if "spot position disagrees" in status.lower():
    sys.exit(3)
PY
    then
      ok=1
      break
    fi
    sleep 0.25
  done
  [[ $ok -eq 1 ]] || fail "Spot qty agreement failed after ${tag} (want ${expect})"
}

clear_50051() {
  if occ=$(ss -lntp 2>/dev/null | grep ':50051' || true); then
    if [[ -n "$occ" ]]; then
      pid=$(echo "$occ" | grep -o 'pid=[0-9]*' | head -1 | cut -d= -f2 || true)
      [[ -n "$pid" ]] || fail "port 50051 occupied: $occ"
      echo "killing leftover pid $pid on 50051"
      kill "$pid" || true
      for _ in $(seq 1 30); do
        ss -lntp 2>/dev/null | grep -q ':50051' || break
        sleep 0.2
      done
    fi
  fi
  if ss -lntp 2>/dev/null | grep -q ':50051'; then
    fail "port 50051 still occupied"
  fi
  if ss -lntp 2>/dev/null | grep -q ":${PORT}"; then
    fail "Paper port ${PORT} occupied"
  fi
}

write_spot_instruments() {
  cat > "$WORK/instruments.yaml" <<'EOF'
instruments:
  - id: BTC-USDT
    symbol: BTC-USDT
    base_asset: BTC
    quote_asset: USDT
    tick_size: 0.01
    lot_size: 0.001
    min_notional: 10
initial_balances:
  - asset: USDT
    amount: 10000000
fill_model:
  kind: immediate_cross
  slippage_bps: 0
  fee_bps: 0
  partial_fills: false
max_trade_history: 10000
market:
  interval_ms: 500
  seed: 7
  step: 100
  spread_half: 50
  levels: 5
  trade_probability: 0.0
  instruments:
    - id: BTC-USDT
      initial_mid: 50000
EOF
}

write_spot_bot() {
  cat > "$WORK/bot.json" <<EOF
{
  "system": { "log_level": "info", "shutdown_timeout_secs": 10 },
  "venue": { "mode": "cex" },
  "exchange_engine": {
    "exchange": "Paper",
    "config": {
      "paper_exchange_url": "ws://127.0.0.1:${PORT}/ws/trade",
      "pairs": [{ "base": "BTC", "quote": "USDT" }],
      "instrument_type": "Spot"
    }
  },
  "server": { "bind_address": "127.0.0.1", "port": 18086 },
  "redis": { "url": "", "enabled": false },
  "features": { "enable_reconciliation": true },
  "reconciliation": {
    "exchange_level_interval_ms": 180000,
    "trading_level_interval_ms": 180000,
    "enabled": true
  },
  "mongo": {
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system_paper_signoff",
    "bot_id": "paper_signoff_spot",
    "enabled": true
  },
  "submission_journal_path": "$WORK/submission-journal.jsonl",
  "execution_ledger_path": "$WORK/execution-ledger.jsonl",
  "shadow_journal_path": "$WORK/shadow-journal.jsonl",
  "runtime_snapshot_path": "$WORK/runtime-snapshot.json",
  "restart_flag_path": "$WORK/restart.requested"
}
EOF
}

write_spot_catalog() {
  cat > "$WORK/levels.json" <<'EOF'
{
  "levels": [
    {
      "id": "so_buy",
      "name": "Signoff spot buy inventory",
      "side": "Buy",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.001" },
      "price": { "Fixed": "10000" },
      "risk": { "max_position": "0.02", "max_orders": 1 },
      "lifecycle": {
        "auto_repost": false,
        "auto_repost_on_cancel": false,
        "max_reposts": 0,
        "repost_delay_secs": 1,
        "repost_conditions": []
      },
      "enabled": false,
      "group": "paper_signoff"
    },
    {
      "id": "so_sell",
      "name": "Signoff spot sell inventory",
      "side": "Sell",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.001" },
      "price": { "Fixed": "100000" },
      "risk": { "max_position": "0.02", "max_orders": 1 },
      "lifecycle": {
        "auto_repost": false,
        "auto_repost_on_cancel": false,
        "max_reposts": 0,
        "repost_delay_secs": 1,
        "repost_conditions": []
      },
      "enabled": false,
      "group": "paper_signoff"
    },
    {
      "id": "so_rest",
      "name": "Signoff disable resting",
      "side": "Buy",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.01" },
      "price": { "Fixed": "1010" },
      "risk": { "max_position": "0.02", "max_orders": 1 },
      "lifecycle": {
        "auto_repost": false,
        "auto_repost_on_cancel": false,
        "max_reposts": 0,
        "repost_delay_secs": 1,
        "repost_conditions": []
      },
      "enabled": false,
      "group": "paper_signoff"
    },
    {
      "id": "so_partial",
      "name": "Signoff delete partial",
      "side": "Buy",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.01" },
      "price": { "Fixed": "1020" },
      "risk": { "max_position": "0.02", "max_orders": 1 },
      "lifecycle": {
        "auto_repost": false,
        "auto_repost_on_cancel": false,
        "max_reposts": 0,
        "repost_delay_secs": 1,
        "repost_conditions": []
      },
      "enabled": false,
      "group": "paper_signoff"
    }
  ],
  "instances": [],
  "tombstones": [],
  "risk_limits": {
    "total_max_position": "0.05",
    "max_total_orders": 8,
    "max_notional": "200",
    "max_order_qty": "0.01",
    "daily_loss_limit_all_pairs": null,
    "daily_loss_limit_per_pair": {}
  },
  "venue": "Paper",
  "instrument": "Spot",
  "account": "paper_signoff_spot"
}
EOF
}

write_futures_bot() {
  cat > "$WORK/bot.json" <<EOF
{
  "system": { "log_level": "info", "shutdown_timeout_secs": 10 },
  "venue": { "mode": "cex" },
  "execution_mode": "exchange",
  "exchange_engine": {
    "exchange": "Paper",
    "config": {
      "paper_exchange_url": "ws://127.0.0.1:${PORT}/ws/trade",
      "pairs": [{ "base": "BTC", "quote": "USDT" }],
      "instrument_type": "Futures",
      "leverage": 10
    }
  },
  "server": { "bind_address": "127.0.0.1", "port": 18086 },
  "redis": { "url": "", "enabled": false },
  "features": { "enable_reconciliation": true },
  "reconciliation": {
    "exchange_level_interval_ms": 180000,
    "trading_level_interval_ms": 180000,
    "enabled": true
  },
  "mongo": {
    "connection_string": "mongodb://localhost:27017",
    "database": "trading_system_paper_signoff",
    "bot_id": "paper_signoff_fut",
    "enabled": true
  },
  "submission_journal_path": "$WORK/fut-submission-journal.jsonl",
  "execution_ledger_path": "$WORK/fut-execution-ledger.jsonl",
  "shadow_journal_path": "$WORK/fut-shadow-journal.jsonl",
  "runtime_snapshot_path": "$WORK/fut-runtime-snapshot.json",
  "restart_flag_path": "$WORK/fut-restart.requested"
}
EOF
}

write_futures_catalog() {
  cat > "$WORK/linear.yaml" <<'EOF'
instruments:
  - id: BTC-USDT
    symbol: BTC-USDT
    base_asset: BTC
    quote_asset: USDT
    tick_size: 0.01
    lot_size: 0.001
    min_notional: 10
    instrument_type: futures
initial_balances:
  - asset: USDT
    amount: 10000000
linear:
  leverage: 10
  taker_fee_bps: 6
  funding_rate: "0"
fill_model:
  kind: immediate_cross
  slippage_bps: 0
  fee_bps: 0
  partial_fills: false
max_trade_history: 10000
market:
  interval_ms: 500
  seed: 7
  step: 100
  spread_half: 50
  levels: 5
  trade_probability: 0.0
  instruments:
    - id: BTC-USDT
      initial_mid: 50000
EOF
  cat > "$WORK/levels.json" <<'EOF'
{
  "levels": [
    {
      "id": "so_fut_buy",
      "name": "Signoff futures buy",
      "side": "Buy",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.001" },
      "price": { "Fixed": "10000" },
      "risk": { "max_position": "0.01", "max_orders": 1 },
      "lifecycle": {
        "auto_repost": false,
        "auto_repost_on_cancel": false,
        "max_reposts": 0,
        "repost_delay_secs": 1,
        "repost_conditions": []
      },
      "enabled": false,
      "group": "paper_signoff_fut"
    }
  ],
  "instances": [],
  "tombstones": [],
  "risk_limits": {
    "total_max_position": "0.01",
    "max_total_orders": 2,
    "max_notional": "200",
    "max_order_qty": "0.001",
    "daily_loss_limit_all_pairs": null,
    "daily_loss_limit_per_pair": {}
  },
  "venue": "Paper",
  "instrument": "Futures",
  "account": "paper_signoff_fut"
}
EOF
}

echo "== build =="
(cd "$ROOT" && cargo build -q -p runner --bin runner -p cli --bin grpc_ctl)
(cd "$KPAPER_DIR" && cargo build -q --bin k-paper)
[[ -x "$RUNNER" ]] || fail "missing $RUNNER"
[[ -x "$GRPC" ]] || fail "missing $GRPC"
[[ -x "$KPAPER_BIN" ]] || fail "missing $KPAPER_BIN"

clear_50051
write_spot_instruments
write_spot_bot
write_spot_catalog

echo "== Spot: inventory buy, restart cost basis, sell, disable, delete =="
start_kpaper "$WORK/instruments.yaml"
start_runner "$WORK/runner-spot.log"
curl -sfS "$HEALTH" | tee "$EV/health_spot_start.txt"
"$GRPC" enable | tee "$EV/grpc_enable_spot.txt"

"$GRPC" toggle-level so_buy_BTCUSDT true | tee "$EV/toggle_buy_on.txt"
BUY_ID=$(wait_open_at 10000 "$EV/rest_buy_open.json")
echo "BUY_ID=$BUY_ID" | tee "$EV/buy_id.txt"
FILL=$(paper_post test/lab_fill_open "{\"qty\":\"0.001\",\"order_id\":\"$BUY_ID\"}")
echo "$FILL" | tee "$EV/lab_buy_fill.json"
assert_spot_qty 0.001 after_buy
paper_get open_orders >"$EV/rest_after_buy.json"

kill_pidfile "$WORK/runner.pid"
sleep 0.3
start_runner "$WORK/runner-spot-restart.log"
"$GRPC" status | tee "$EV/grpc_status_after_restart.txt"
assert_spot_qty 0.001 after_restart
python3 - <<'PY'
import re,sys
status=open("/tmp/paper-signoff/evidence/grpc_status_after_restart.txt").read()
m=re.search(r"position BTCUSDT qty=([0-9.]+) side=([^ ]*) entry=([^ ]+)", status)
if not m:
    sys.exit("restart status missing position line")
qty,entry=m.group(1),m.group(3)
print("restart position", qty, "entry", entry)
if abs(float(qty)-0.001)>1e-6:
    sys.exit(f"restart qty {qty}")
if entry not in ("?",):
    sys.exit(f"restart entry should be unknown ?, got {entry!r}")
PY

"$GRPC" enable | tee "$EV/grpc_enable_after_restart.txt"
"$GRPC" toggle-level so_sell_BTCUSDT true | tee "$EV/toggle_sell_on.txt"
SELL_ID=$(wait_open_at 100000 "$EV/rest_sell_open.json")
echo "SELL_ID=$SELL_ID" | tee "$EV/sell_id.txt"
SFILL=$(paper_post test/lab_fill_open "{\"qty\":\"0.001\",\"order_id\":\"$SELL_ID\"}")
echo "$SFILL" | tee "$EV/lab_sell_fill.json"
assert_spot_qty 0 after_sell
paper_get open_orders >"$EV/rest_after_sell.json"
paper_get balances >"$EV/balances_after_sell.json"

"$GRPC" toggle-level so_rest_BTCUSDT true | tee "$EV/toggle_rest_on.txt"
REST_ID=$(wait_open_at 1010 "$EV/rest_disable_open.json")
echo "REST_ID=$REST_ID" | tee "$EV/rest_id.txt"
"$GRPC" status | tee "$EV/grpc_status_before_disable.txt"
"$GRPC" toggle-level so_rest_BTCUSDT false | tee "$EV/toggle_rest_off.txt"
ok=0
for _ in $(seq 1 20); do
  paper_get open_orders >"$EV/rest_after_disable.json"
  if python3 - <<PY
import json,sys
orders=json.load(open("/tmp/paper-signoff/evidence/rest_after_disable.json"))
if any(o.get("orderId")=="$REST_ID" and o.get("status") in ("New","PartiallyFilled") for o in orders):
    sys.exit(2)
print("disable REST remaining", [o["orderId"] for o in orders if o.get("status") in ("New","PartiallyFilled")])
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "REST still has resting order after disable"
"$GRPC" levels | tee "$EV/grpc_levels_after_disable.txt"
grep -q "so_rest_BTCUSDT enabled=false" "$EV/grpc_levels_after_disable.txt" || fail "level not disabled"
"$GRPC" status | tee "$EV/grpc_status_after_disable.txt"
grep -q "$REST_ID" "$EV/grpc_status_after_disable.txt" && grep -q "status=New" "$EV/grpc_status_after_disable.txt" && fail "runner still shows resting New after disable" || true

"$GRPC" toggle-level so_partial_BTCUSDT true | tee "$EV/toggle_partial_on.txt"
PART_ID=$(wait_open_at 1020 "$EV/rest_partial_open.json")
echo "PART_ID=$PART_ID" | tee "$EV/partial_id.txt"
PFILL=$(paper_post test/lab_fill_open "{\"qty\":\"0.001\",\"order_id\":\"$PART_ID\"}")
echo "$PFILL" | tee "$EV/lab_partial_fill.json"
assert_spot_qty 0.001 after_partial
"$GRPC" delete-level so_partial_BTCUSDT | tee "$EV/grpc_delete_partial.txt"
ok=0
for _ in $(seq 1 20); do
  paper_get open_orders >"$EV/rest_after_delete.json"
  if python3 - <<PY
import json,sys
orders=json.load(open("/tmp/paper-signoff/evidence/rest_after_delete.json"))
if any(o.get("orderId")=="$PART_ID" and o.get("status") in ("New","PartiallyFilled") for o in orders):
    sys.exit(2)
print("delete REST remaining working", [o["orderId"] for o in orders if o.get("status") in ("New","PartiallyFilled")])
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "REST still has partial order after delete"
"$GRPC" levels | tee "$EV/grpc_levels_after_delete.txt"
grep -q "so_partial_BTCUSDT" "$EV/grpc_levels_after_delete.txt" && fail "deleted level still listed" || true
cp "$WORK/levels.json" "$EV/catalog_after_delete.json"
python3 - <<'PY'
import json,sys
c=json.load(open("/tmp/paper-signoff/evidence/catalog_after_delete.json"))
tomb=c.get("tombstones") or []
print("tombstones", tomb)
if not any("so_partial" in t for t in tomb):
    sys.exit("catalog missing so_partial tombstone")
ids=[i["id"] for i in c.get("instances") or []]
if any("so_partial" in i for i in ids):
    sys.exit(f"deleted instance still present {ids}")
PY
"$GRPC" status | tee "$EV/grpc_status_after_delete.txt"
assert_spot_qty 0.001 after_delete

kill_pidfile "$WORK/runner.pid"
kill_pidfile "$WORK/k-paper.pid"
sleep 0.4
clear_50051

echo "== Futures: rest, fill, REST position vs runner, disable =="
write_futures_bot
write_futures_catalog
start_kpaper "$WORK/linear.yaml"
start_runner "$WORK/runner-fut.log"
curl -sfS "$HEALTH" | tee "$EV/health_fut_start.txt"
"$GRPC" enable | tee "$EV/grpc_enable_fut.txt"
"$GRPC" toggle-level so_fut_buy_BTCUSDT true | tee "$EV/toggle_fut_on.txt"
FUT_ID=$(wait_open_at 10000 "$EV/rest_fut_open.json")
echo "FUT_ID=$FUT_ID" | tee "$EV/fut_id.txt"
paper_get open_orders >"$EV/rest_fut_before_fill.json"
FFILL=$(paper_post test/lab_fill_open "{\"qty\":\"0.001\",\"order_id\":\"$FUT_ID\"}")
echo "$FFILL" | tee "$EV/lab_fut_fill.json"
ok=0
for _ in $(seq 1 40); do
  paper_get positions >"$EV/rest_fut_positions.json" || echo '[]' >"$EV/rest_fut_positions.json"
  "$GRPC" status >"$EV/grpc_status_fut_fill.txt" 2>/dev/null || true
  if python3 - <<'PY'
import json,sys,re
from pathlib import Path
ev=Path("/tmp/paper-signoff/evidence")
eps=1e-6
raw=json.loads((ev/"rest_fut_positions.json").read_text() or "[]")
rows=raw if isinstance(raw,list) else raw.get("positions",[])
rest=0.0
for p in rows:
    inst=str(p.get("instrument_id") or p.get("symbol") or "")
    if "BTC" in inst.upper():
        rest=float(p.get("quantity") or p.get("qty") or 0)
        break
status=(ev/"grpc_status_fut_fill.txt").read_text()
m=re.search(r"position BTCUSDT qty=([0-9.-]+)", status)
runner=float(m.group(1)) if m else 0.0
print("fut-agree", {"rest":rest,"runner":runner})
if abs(rest-0.001)>eps or abs(runner-0.001)>eps:
    sys.exit(2)
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "Futures REST position vs runner disagreement after fill"

"$GRPC" toggle-level so_fut_buy_BTCUSDT false | tee "$EV/toggle_fut_off.txt"
ok=0
for _ in $(seq 1 20); do
  paper_get open_orders >"$EV/rest_fut_after_disable.json"
  if python3 - <<PY
import json,sys
orders=json.load(open("/tmp/paper-signoff/evidence/rest_fut_after_disable.json"))
working=[o for o in orders if o.get("status") in ("New","PartiallyFilled")]
print("fut working after disable", [o["orderId"] for o in working])
sys.exit(0 if not working else 2)
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "Futures working orders remain after disable"
"$GRPC" levels | tee "$EV/grpc_levels_fut_after_disable.txt"
grep -q "so_fut_buy_BTCUSDT enabled=false" "$EV/grpc_levels_fut_after_disable.txt" || fail "futures level not disabled"
paper_get positions >"$EV/rest_fut_positions_after_disable.json" || true
"$GRPC" status | tee "$EV/grpc_status_fut_after_disable.txt"

cp "$WORK/runner-spot.log" "$EV/runner-spot.log"
cp "$WORK/runner-spot-restart.log" "$EV/runner-spot-restart.log"
cp "$WORK/runner-fut.log" "$EV/runner-fut.log"
cp "$WORK/k-paper.log" "$EV/k-paper.log"
cp "$WORK/execution-ledger.jsonl" "$EV/execution-ledger.jsonl" 2>/dev/null || true
cp "$WORK/fut-execution-ledger.jsonl" "$EV/fut-execution-ledger.jsonl" 2>/dev/null || true
echo PASS | tee "$EV/RESULT.txt"

rm -rf "$PACK"
mkdir -p "$PACK"
cp -a "$EV"/. "$PACK"/
{
  echo "# Paper sign-off remaining-path evidence"
  echo
  echo "Isolated Paper on :${PORT}. Does not modify /tmp/paper-levels-evidence-f02ca01 or :19996."
  echo "Reused: docs/paper-ack-amend-evidence (ACK/amend, fill-once, restart qty)."
  echo "Reused: docs/paper-own-fix-evidence (two-level ownership)."
  echo
  echo "- Paper HEAD: $(cd "$KPAPER_DIR" && git rev-parse HEAD)"
  echo "- trading_system HEAD: $(cd "$ROOT" && git rev-parse HEAD)"
  echo "- RESULT: PASS"
} | tee "$PACK/README.md"
(cd "$PACK" && sha256sum * >SHA256SUMS 2>/dev/null || true)
echo "evidence pack: $PACK"
