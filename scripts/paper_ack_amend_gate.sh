#!/usr/bin/env bash
# Paper ACK correlation, partial-fill replay, and capped price-only amend.
# Isolated Paper. Does not touch /tmp/paper-levels-evidence-f02ca01 or :19996.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KPAPER_DIR="${K_PAPER_DIR:-/home/kolzo-dev/Desktop/k-paper}"
WORK="${PAPER_ACK_AMEND_WORKDIR:-/tmp/paper-ack-amend}"
EV="$WORK/evidence"
PORT="${PAPER_EXCHANGE_PORT:-19994}"
HTTP="http://127.0.0.1:${PORT}"
HEALTH="http://127.0.0.1:18086/health"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
RUNNER="$TARGET_DIR/debug/runner"
GRPC="$TARGET_DIR/debug/grpc_ctl"
KPAPER_TARGET="${CARGO_TARGET_DIR:-$KPAPER_DIR/target}"
KPAPER_BIN="$KPAPER_TARGET/debug/k-paper"
PACK="$ROOT/docs/paper-ack-amend-evidence"

export TRACE_JAEGER_DISABLED="${TRACE_JAEGER_DISABLED:-1}"
export TRADING_GRPC="${TRADING_GRPC:-http://127.0.0.1:50051}"

mkdir -p "$WORK" "$EV"
rm -f "$WORK"/runner*.log "$WORK"/k-paper.log "$WORK"/*.jsonl "$WORK"/runtime-snapshot.json

fail() { echo "FAILED: $*" >&2; exit 1; }

# Paper BTC, runner position, catalog filled_qty, unique ledger qty must match.
assert_fill_agreement() {
  local expect=$1
  local tag=$2
  local ok=0
  local i
  for i in $(seq 1 40); do
    paper_get balances >"$EV/balances_${tag}.json" || true
    cp "$WORK/levels.json" "$EV/catalog_${tag}.json"
    "$GRPC" status >"$EV/grpc_status_${tag}.txt" 2>/dev/null || true
    if [[ -f "$WORK/execution-ledger.jsonl" ]]; then
      cp "$WORK/execution-ledger.jsonl" "$EV/execution-ledger.jsonl"
    fi
    if python3 - "$expect" "$tag" <<'PY'
import json,sys,re
from pathlib import Path
expect=float(sys.argv[1])
tag=sys.argv[2]
ev=Path("/tmp/paper-ack-amend/evidence")
eps=1e-6

def fnum(v):
    try:
        return float(v)
    except Exception:
        return None

paper_btc=0.0
bp=ev/f"balances_{tag}.json"
if bp.exists() and bp.stat().st_size>0:
    raw=json.loads(bp.read_text() or "[]")
    rows=raw if isinstance(raw,list) else raw.get("balances",[])
    for b in rows:
        asset=str(b.get("asset") or b.get("symbol") or "")
        if asset.upper() in ("BTC","XBT"):
            paper_btc=float(b.get("total") or b.get("available") or 0)
            break

status=(ev/f"grpc_status_{tag}.txt").read_text() if (ev/f"grpc_status_{tag}.txt").exists() else ""
m=re.search(r"position BTCUSDT qty=([0-9.]+)", status)
runner=float(m.group(1)) if m else 0.0

catalog_qty=0.0
cat=json.loads((ev/f"catalog_{tag}.json").read_text())
for inst in cat.get("instances") or []:
    catalog_qty += float(inst.get("filled_qty") or 0)

ledger_qty=0.0
seen=set()
lp=ev/"execution-ledger.jsonl"
if lp.exists():
    for line in lp.read_text().splitlines():
        if not line.strip():
            continue
        rec=json.loads(line)
        fill=rec.get("Fill") or (rec if rec.get("kind")=="fill" else None)
        if not fill:
            continue
        eid=str(fill.get("execution_id") or "")
        if not eid or eid in seen:
            continue
        seen.add(eid)
        ledger_qty += float(fill.get("quantity") or 0)

vals={"paper_btc":paper_btc,"runner":runner,"catalog":catalog_qty,"ledger":ledger_qty}
print(f"fill-agree {tag}", vals, "expect", expect)
for k,v in vals.items():
    if abs(v-expect)>eps:
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
  [[ $ok -eq 1 ]] || fail "position agreement failed after ${tag} (want ${expect})"
}

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
  if [[ "${PAPER_ACK_AMEND_KEEP:-}" == "1" ]]; then
    echo "KEEP=1 leaving Paper pid=$(cat "$WORK/k-paper.pid" 2>/dev/null || true) runner pid=$(cat "$WORK/runner.pid" 2>/dev/null || true)"
    return 0
  fi
  kill_pidfile "$WORK/runner.pid"
  kill_pidfile "$WORK/k-paper.pid"
}
trap cleanup EXIT

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

write_bot() {
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
    "database": "trading_system_paper_ack_amend",
    "bot_id": "paper_ack_amend",
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

write_two_level_catalog() {
  cat > "$WORK/levels.json" <<'EOF'
{
  "levels": [
    {
      "id": "qual_rest_a",
      "name": "Paper spot resting A",
      "side": "Buy",
      "order_type": "Limit",
      "quantity": { "type": "Fixed", "value": "0.01" },
      "price": { "Fixed": "1000" },
      "risk": { "max_position": "0.02", "max_orders": 1 },
      "lifecycle": {
        "auto_repost": false,
        "auto_repost_on_cancel": false,
        "max_reposts": 0,
        "repost_delay_secs": 1,
        "repost_conditions": []
      },
      "enabled": false,
      "group": "paper_ack_amend"
    },
    {
      "id": "qual_rest_b",
      "name": "Paper spot resting B",
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
      "group": "paper_ack_amend"
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
  "account": "paper_ack_amend"
}
EOF
}

write_capped_amend_catalog() {
  cat > "$WORK/levels.json" <<'EOF'
{
  "levels": [
    {
      "id": "qual_amend",
      "name": "Price-only amend at exact caps",
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
      "group": "paper_ack_amend"
    }
  ],
  "instances": [],
  "tombstones": [],
  "risk_limits": {
    "total_max_position": "0.02",
    "max_total_orders": 1,
    "max_notional": "200",
    "max_order_qty": "0.01",
    "daily_loss_limit_all_pairs": null,
    "daily_loss_limit_per_pair": {}
  },
  "venue": "Paper",
  "instrument": "Spot",
  "account": "paper_ack_amend"
}
EOF
}

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

echo "== build =="
(cd "$ROOT" && cargo build -q -p runner --bin runner -p cli --bin grpc_ctl)
(cd "$KPAPER_DIR" && cargo build -q --bin k-paper)
[[ -x "$RUNNER" ]] || fail "missing $RUNNER"
[[ -x "$GRPC" ]] || fail "missing $GRPC"
[[ -x "$KPAPER_BIN" ]] || fail "missing $KPAPER_BIN"

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

write_bot
write_two_level_catalog

echo "starting k-paper on :${PORT}"
PAPER_EXCHANGE_PORT="${PORT}" \
PAPER_EXCHANGE_CONFIG="$WORK/instruments.yaml" \
PAPER_EXCHANGE_PROFILE=lab \
"$KPAPER_BIN" >"$WORK/k-paper.log" 2>&1 &
echo $! >"$WORK/k-paper.pid"
wait_http "$HTTP/open_orders"

start_runner "$WORK/runner.log"
curl -sfS "$HEALTH" | tee "$EV/health_start.txt"
"$GRPC" enable | tee "$EV/grpc_enable.txt"

"$GRPC" toggle-level qual_rest_a_BTCUSDT true | tee "$EV/toggle_a_on.txt"
A_ID=""
for _ in $(seq 1 40); do
  paper_get open_orders >"$EV/rest_after_a.json"
  A_ID=$(python3 - <<'PY'
import json
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_a.json"))
for o in orders:
    if str(o.get("price")).startswith("1000"):
        print(o["orderId"]); break
PY
)
  if [[ -n "$A_ID" ]]; then
    break
  fi
  sleep 0.25
done
[[ -n "$A_ID" ]] || fail "level A did not rest"
echo "A_ID=$A_ID" | tee "$EV/a_id.txt"

paper_post test/suppress_place_ack '{}' >/dev/null || fail "suppress_place_ack"
"$GRPC" toggle-level qual_rest_b_BTCUSDT true | tee "$EV/toggle_b_on.txt"
B_ID=""
for _ in $(seq 1 40); do
  paper_get open_orders >"$EV/rest_two_open.json"
  B_ID=$(python3 - <<PY
import json
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_two_open.json"))
a="$A_ID"
for o in orders:
    if o["orderId"] != a:
        print(o["orderId"]); break
PY
)
  if [[ -n "$B_ID" ]]; then
    break
  fi
  sleep 0.25
done
[[ -n "$B_ID" ]] || fail "level B did not book while ACK suppressed"
[[ "$A_ID" != "$B_ID" ]] || fail "A and B share exchange id at booking"
echo "B_ID=$B_ID" | tee "$EV/b_id.txt"
cp "$WORK/levels.json" "$EV/catalog_before_wrong_ack.json"

paper_post test/emit_place_ack "{\"order_id\":\"$A_ID\",\"omit_client_order_id\":true}" \
  >/dev/null || fail "emit uncorrelated A ACK"
sleep 0.4
paper_post test/emit_place_ack "{\"order_id\":\"$A_ID\",\"omit_client_order_id\":false}" \
  >/dev/null || fail "emit correlated-but-wrong A ACK"
sleep 0.4
cp "$WORK/levels.json" "$EV/catalog_after_wrong_ack.json"
python3 - <<PY
import json,sys
c=json.load(open("/tmp/paper-ack-amend/evidence/catalog_after_wrong_ack.json"))
inst={i["id"]:i for i in c["instances"]}
a=inst["qual_rest_a_BTCUSDT"]
b=inst["qual_rest_b_BTCUSDT"]
print("after wrong ACK A", a.get("owned_exchange_ids"), "inflight", a.get("inflight_client_order_id"))
print("after wrong ACK B", b.get("owned_exchange_ids"), "inflight", b.get("inflight_client_order_id"))
if "$B_ID" in (a.get("owned_exchange_ids") or []):
    sys.exit("A claimed B after delayed A ACK")
if "$A_ID" in (b.get("owned_exchange_ids") or []):
    sys.exit("B attached A's exchange id from delayed ACK")
PY

paper_post test/emit_place_ack "{\"order_id\":\"$B_ID\",\"omit_client_order_id\":false}" \
  >/dev/null || fail "emit B ACK"
ok=0
for _ in $(seq 1 40); do
  cp "$WORK/levels.json" "$EV/catalog_after_b_ack.json"
  if python3 - <<PY
import json,sys
c=json.load(open("/tmp/paper-ack-amend/evidence/catalog_after_b_ack.json"))
b=next(i for i in c["instances"] if i["id"]=="qual_rest_b_BTCUSDT")
ids=b.get("owned_exchange_ids") or []
print("B after own ACK", ids, b.get("state"))
if ids != ["$B_ID"]:
    sys.exit(2)
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "B did not attach its own exchange id after correlated ACK"
"$GRPC" levels | tee "$EV/grpc_levels_after_wrong_ack.txt"
grep -nE "client_order_id=None|verifying place ACK|execution correlation fields|place response is not correlated|place ACK exchange id|client-id lookup inconclusive|does not complete pending" \
  "$WORK/runner.log" | tee "$EV/runner_wrong_ack.txt" || true

echo "== partial fill + replay + restart =="
FILL=$(paper_post test/lab_fill_open "{\"qty\":\"0.001\",\"order_id\":\"$A_ID\"}")
echo "$FILL" | tee "$EV/lab_partial_fill_a.json"
echo "$FILL" | grep -q "$A_ID" || fail "partial fill did not target A"
ok=0
for _ in $(seq 1 20); do
  paper_get open_orders >"$EV/rest_after_partial.json"
  if python3 - <<'PY'
import json,sys
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_partial.json"))
a=[o for o in orders if o["orderId"]==open("/tmp/paper-ack-amend/evidence/a_id.txt").read().split("=",1)[-1].strip()]
if not a:
    sys.exit(2)
o=a[0]
print("partial REST", o["status"], o["filledAmount"], o["amount"])
if o["status"] != "PartiallyFilled":
    sys.exit(3)
if not str(o["filledAmount"]).startswith("0.001"):
    sys.exit(4)
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "REST did not show PartiallyFilled 0.001"
"$GRPC" status | tee "$EV/grpc_status_after_partial.txt"
grep -q "PartiallyFilled" "$EV/grpc_status_after_partial.txt" || fail "grpc status missing PartiallyFilled"
assert_fill_agreement 0.001 partial

DUP=$(paper_post test/lab_fill_open "{\"qty\":\"0.001\",\"order_id\":\"$A_ID\"}")
echo "$DUP" | tee "$EV/lab_partial_replay.json"
paper_get open_orders >"$EV/rest_after_replay.json"
python3 - <<PY
import json,sys
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_replay.json"))
a=[o for o in orders if o["orderId"]=="$A_ID"]
if not a:
    sys.exit("A missing after replay")
o=a[0]
print("replay REST", o["status"], o["filledAmount"])
filled=float(o["filledAmount"])
if filled > 0.0020001:
    sys.exit(f"replay over-filled A to {filled}")
PY
cp "$WORK/levels.json" "$EV/catalog_after_partial.json"
"$GRPC" status | tee "$EV/grpc_status_after_replay.txt"
assert_fill_agreement 0.002 replay

kill_pidfile "$WORK/runner.pid"
sleep 0.3
start_runner "$WORK/runner-restart.log"
sleep 1
cp "$WORK/levels.json" "$EV/catalog_after_restart.json"
paper_get open_orders >"$EV/rest_after_restart.json"
"$GRPC" status | tee "$EV/grpc_status_after_restart.txt"
"$GRPC" levels | tee "$EV/grpc_levels_after_restart.txt"
python3 - <<PY
import json,sys
rest=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_restart.json"))
a=[o for o in rest if o["orderId"]=="$A_ID"]
b=[o for o in rest if o["orderId"]=="$B_ID"]
print("restart REST A", a[0] if a else None)
print("restart REST B", b[0] if b else None)
if not a:
    sys.exit("A missing on Paper after restart")
if a[0]["status"] != "PartiallyFilled":
    sys.exit(f"A status after restart {a[0]['status']}")
filled=float(a[0]["filledAmount"])
if filled < 0.0019 or filled > 0.0020001:
    sys.exit(f"A filledAmount after restart {a[0]['filledAmount']} (want post-replay 0.002)")
if not b:
    sys.exit("B missing on Paper after restart")
c=json.load(open("/tmp/paper-ack-amend/evidence/catalog_after_restart.json"))
inst={i["id"]:i for i in c["instances"]}
owned_a=inst["qual_rest_a_BTCUSDT"].get("owned_exchange_ids") or []
owned_b=inst["qual_rest_b_BTCUSDT"].get("owned_exchange_ids") or []
if "$A_ID" not in owned_a:
    sys.exit(f"catalog A lost {owned_a}")
if "$B_ID" not in owned_b:
    sys.exit(f"catalog B lost {owned_b}")
if "$A_ID" in owned_b or "$B_ID" in owned_a:
    sys.exit("restart mixed A/B ownership")
PY
grep -q "PartiallyFilled" "$EV/grpc_status_after_restart.txt" || fail "grpc after restart missing partial"
assert_fill_agreement 0.002 restart

echo "== cancel leftover working orders =="
"$GRPC" cancel paper-ack-amend-cancel | tee "$EV/grpc_cancel.txt" || true
ok=0
for _ in $(seq 1 20); do
  paper_get open_orders >"$EV/rest_after_cancel.json"
  if python3 - <<'PY'
import json,sys
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_cancel.json"))
working=[o for o in orders if o.get("status") in ("New","PartiallyFilled")]
print("working after cancel", [o["orderId"] for o in working])
sys.exit(0 if not working else 2)
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "working orders remain after cancel"

kill_pidfile "$WORK/runner.pid"
sleep 0.3
write_capped_amend_catalog
start_runner "$WORK/runner-amend.log"
"$GRPC" enable | tee "$EV/grpc_enable_amend.txt"
"$GRPC" toggle-level qual_amend_BTCUSDT true | tee "$EV/toggle_amend_on.txt"

AMEND_ID=""
for _ in $(seq 1 40); do
  paper_get open_orders >"$EV/rest_before_amend.json"
  AMEND_ID=$(python3 - <<'PY'
import json
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_before_amend.json"))
for o in orders:
    if str(o.get("price")).startswith("1010"):
        print(o["orderId"]); break
PY
)
  if [[ -n "$AMEND_ID" ]]; then
    break
  fi
  sleep 0.25
done
[[ -n "$AMEND_ID" ]] || fail "capped amend level did not rest at 1010"
echo "AMEND_ID=$AMEND_ID" | tee "$EV/amend_id.txt"
"$GRPC" status | tee "$EV/grpc_status_before_amend.txt"

AMEND_LOGS_BEFORE=$(grep -c "Batch amending" "$WORK/runner-amend.log" || true)
"$GRPC" update-level qual_amend_BTCUSDT "price=Fixed(1020)" | tee "$EV/update_amend.txt"
ok=0
for _ in $(seq 1 30); do
  paper_get open_orders >"$EV/rest_after_price_amend.json"
  if python3 - <<PY
import json,sys
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_price_amend.json"))
o=[x for x in orders if x["orderId"]=="$AMEND_ID"]
if not o:
    sys.exit(2)
print("amend REST", o[0]["orderId"], o[0]["price"], o[0]["amount"], o[0]["status"])
if not str(o[0]["price"]).startswith("1020"):
    sys.exit(3)
if not str(o[0]["amount"]).startswith("0.01"):
    sys.exit(4)
PY
  then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "Paper REST did not show 1020/0.01 after price-only amend"
ok=0
for _ in $(seq 1 20); do
  "$GRPC" status | tee "$EV/grpc_status_after_amend.txt"
  if grep -E "px=1020" "$EV/grpc_status_after_amend.txt" >/dev/null; then
    ok=1
    break
  fi
  sleep 0.25
done
[[ $ok -eq 1 ]] || fail "grpc runner state missing price 1020"
python3 - <<PY
import json,sys,re
status=open("/tmp/paper-ack-amend/evidence/grpc_status_after_amend.txt").read()
rest=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_price_amend.json"))[0]
print("compare REST vs grpc", rest["price"], rest["amount"])
if "$AMEND_ID" not in status:
    sys.exit("grpc status missing amend order id")
if "0.01" not in status:
    sys.exit("grpc status missing qty 0.01")
PY
cp "$WORK/levels.json" "$EV/catalog_after_price_amend.json"
python3 - <<PY
import json,sys
c=json.load(open("/tmp/paper-ack-amend/evidence/catalog_after_price_amend.json"))
lvl=next(i for i in c["instances"] if i["id"]=="qual_amend_BTCUSDT")
ids=lvl.get("owned_exchange_ids") or []
print("amend catalog", ids, lvl.get("config",{}).get("price"))
if ids != ["$AMEND_ID"]:
    sys.exit(f"ownership changed on amend: {ids}")
price=lvl.get("config",{}).get("price")
if price != {"Fixed": "1020"} and "1020" not in str(price):
    sys.exit(f"catalog price not 1020: {price}")
PY

sleep 2
AMEND_LOGS_AFTER=$(grep -c "Batch amending" "$WORK/runner-amend.log" || true)
echo "amend_batches_before=$AMEND_LOGS_BEFORE after=$AMEND_LOGS_AFTER" | tee "$EV/amend_batch_counts.txt"
python3 - <<PY
before=int("$AMEND_LOGS_BEFORE" or 0)
after=int("$AMEND_LOGS_AFTER" or 0)
print("amend batches", before, "->", after)
if after - before > 1:
    raise SystemExit(f"subsequent ticks repeated amend ({after-before} batches)")
PY
paper_get open_orders >"$EV/rest_after_wait.json"
python3 - <<PY
import json,sys
orders=json.load(open("/tmp/paper-ack-amend/evidence/rest_after_wait.json"))
if len(orders)!=1:
    sys.exit(f"expected one working order after wait, got {orders}")
o=orders[0]
if o["orderId"]!="$AMEND_ID":
    sys.exit("order id changed after wait")
if not str(o["price"]).startswith("1020"):
    sys.exit("price drifted after wait")
PY

cp "$WORK/levels.json" "$EV/catalog_final.json"
cp "$WORK/submission-journal.jsonl" "$EV/submission-journal.jsonl"
cp "$WORK/execution-ledger.jsonl" "$EV/execution-ledger.jsonl" 2>/dev/null || true
cp "$WORK/runner.log" "$EV/runner.log"
cp "$WORK/runner-restart.log" "$EV/runner-restart.log"
cp "$WORK/runner-amend.log" "$EV/runner-amend.log"
cp "$WORK/k-paper.log" "$EV/k-paper.log"
"$GRPC" levels | tee "$EV/grpc_levels_final.txt"
"$GRPC" status | tee "$EV/grpc_status_final.txt"
echo PASS | tee "$EV/RESULT.txt"

rm -rf "$PACK"
mkdir -p "$PACK"
cp -a "$EV"/. "$PACK"/
{
  echo "# Paper ACK / amend gate evidence"
  echo
  echo "Isolated Paper on :${PORT}. trading_system and k-paper SHAs recorded below."
  echo "Previous two-level own-fix pack copied to docs/paper-own-fix-evidence/."
  echo "Archive /tmp/paper-levels-evidence-f02ca01 was not modified."
  echo
  echo "- Paper HEAD: $(cd "$KPAPER_DIR" && git rev-parse HEAD)"
  echo "- trading_system HEAD: $(cd "$ROOT" && git rev-parse HEAD)"
  echo "- RESULT: PASS"
} >"$PACK/README.md"
(cd "$PACK" && sha256sum * >SHA256SUMS)
echo "evidence pack: $PACK"
echo "A=$A_ID B=$B_ID AMEND=$AMEND_ID"
