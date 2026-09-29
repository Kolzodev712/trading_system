#!/usr/bin/env python3
"""Independent Bybit Demo REST snapshot vs grpc_ctl status.

Reads demo:true credentials from a bot.json. Does not place, cancel, or flatten.
"""
from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from decimal import Decimal, InvalidOperation
from pathlib import Path

DEMO_REST = "https://api-demo.bybit.com"


def qty_norm(value) -> Decimal:
    try:
        return Decimal(str(value if value is not None else "0"))
    except (InvalidOperation, ValueError):
        return Decimal("0")


def side_norm(value: str) -> str:
    s = str(value or "").strip().lower()
    if s in ("buy", "long"):
        return "buy"
    if s in ("sell", "short"):
        return "sell"
    return s


def symbol_norm(value: str) -> str:
    return str(value or "").replace("/", "").replace("-", "").upper()


def signed_get(key: str, secret: str, path: str, params: dict) -> dict:
    qs = urllib.parse.urlencode(params)
    ts = str(int(time.time() * 1000))
    recv = "5000"
    payload = ts + key + recv + qs
    sig = hmac.new(secret.encode(), payload.encode(), hashlib.sha256).hexdigest()
    req = urllib.request.Request(
        f"{DEMO_REST}{path}?{qs}",
        headers={
            "X-BAPI-API-KEY": key,
            "X-BAPI-TIMESTAMP": ts,
            "X-BAPI-RECV-WINDOW": recv,
            "X-BAPI-SIGN": sig,
        },
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        body = json.loads(resp.read().decode())
    if body.get("retCode") != 0:
        raise SystemExit(
            f"FAILED: Bybit demo {path} retCode={body.get('retCode')} retMsg={body.get('retMsg')}"
        )
    return body.get("result") or {}


def parse_grpc_status(status: str) -> tuple[int, int, str, list[dict]]:
    grpc_orders = 0
    m = re.search(r"orders=(\d+)", status)
    if m:
        grpc_orders = int(m.group(1))
    grpc_pos_n = 0
    m = re.search(r"positions=(\d+)", status)
    if m:
        grpc_pos_n = int(m.group(1))
    strat_pos = "0"
    m = re.search(r"strategy .* pos=([0-9.-]+)", status)
    if m:
        strat_pos = m.group(1)
    grpc_positions = []
    for m in re.finditer(r"position (\S+) qty=([0-9.-]+) side=(\S+)", status):
        qty = qty_norm(m.group(2))
        if qty == 0:
            continue
        grpc_positions.append(
            {
                "symbol": symbol_norm(m.group(1)),
                "side": side_norm(m.group(3)),
                "qty": str(qty),
            }
        )
    grpc_positions.sort(key=lambda r: (r["symbol"], r["side"], r["qty"]))
    return grpc_orders, grpc_pos_n, strat_pos, grpc_positions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bot_json", type=Path, help="Demo bot.json with credentials")
    parser.add_argument(
        "--grpc-status",
        default="",
        help="Raw grpc_ctl status text (omit to print REST only)",
    )
    parser.add_argument(
        "--require-flat",
        action="store_true",
        help="Exit non-zero unless Demo REST and runner are zero orders / zero position",
    )
    parser.add_argument("--out", type=Path, help="Write JSON snapshot")
    args = parser.parse_args()

    bot = json.loads(args.bot_json.read_text())
    eng = bot.get("exchange_engine") or {}
    if str(eng.get("exchange", "")).strip().lower() != "bybit":
        raise SystemExit("FAILED: bot must use exchange_engine.exchange=Bybit")
    cfg = eng.get("config") or {}
    if cfg.get("demo") is not True:
        raise SystemExit("FAILED: bot must set exchange_engine.config.demo=true")
    if cfg.get("testnet") is True:
        raise SystemExit("FAILED: bot must not set testnet when demo=true")
    cred = cfg.get("credentials") or {}
    key, secret = cred.get("apiKey") or "", cred.get("apiSecret") or ""
    if not key or "REPLACE" in key:
        raise SystemExit("FAILED: Demo API key is still a placeholder")

    pos = signed_get(key, secret, "/v5/position/list", {"category": "linear", "symbol": "BTCUSDT"})
    orders = signed_get(
        key, secret, "/v5/order/realtime", {"category": "linear", "symbol": "BTCUSDT"}
    )
    venue_positions = []
    for p in pos.get("list") or []:
        qty = qty_norm(p.get("size") or "0")
        if qty == 0:
            continue
        venue_positions.append(
            {
                "symbol": symbol_norm(p.get("symbol")),
                "side": side_norm(p.get("side")),
                "qty": str(qty),
            }
        )
    venue_positions.sort(key=lambda r: (r["symbol"], r["side"], r["qty"]))
    order_rows = orders.get("list") or []
    snapshot = {
        "demo_open_orders": len(order_rows),
        "demo_positions": venue_positions,
    }
    if args.grpc_status:
        grpc_orders, grpc_pos_n, strat_pos, grpc_positions = parse_grpc_status(args.grpc_status)
        snapshot.update(
            {
                "grpc_orders": grpc_orders,
                "grpc_positions_n": grpc_pos_n,
                "grpc_positions": grpc_positions,
                "grpc_strategy_pos": strat_pos,
            }
        )
        if len(order_rows) != grpc_orders:
            raise SystemExit(
                f"FAILED: demo REST open orders {len(order_rows)} != grpc orders={grpc_orders}"
            )
        if [ (r["symbol"], r["side"], qty_norm(r["qty"])) for r in venue_positions ] != [
            (r["symbol"], r["side"], qty_norm(r["qty"])) for r in grpc_positions
        ]:
            raise SystemExit(
                f"FAILED: demo REST positions {venue_positions} != grpc positions {grpc_positions}"
            )
        if len(venue_positions) != grpc_pos_n:
            raise SystemExit(
                f"FAILED: demo REST position count {len(venue_positions)} != grpc positions={grpc_pos_n}"
            )
        net = Decimal("0")
        for row in venue_positions:
            q = qty_norm(row["qty"])
            net += q if row["side"] == "buy" else -q
        if qty_norm(strat_pos) != net:
            raise SystemExit(f"FAILED: strategy pos={strat_pos} != demo REST net qty {net}")
        if args.require_flat:
            if (
                order_rows
                or venue_positions
                or grpc_orders
                or grpc_positions
                or qty_norm(strat_pos) != 0
            ):
                raise SystemExit(
                    "FAILED: account is not independently flat "
                    f"(rest_orders={len(order_rows)} rest_pos={venue_positions} "
                    f"grpc_orders={grpc_orders} grpc_pos={grpc_positions} strat={strat_pos})"
                )
    elif args.require_flat:
        if order_rows or venue_positions:
            raise SystemExit(
                f"FAILED: Demo REST is not flat (orders={len(order_rows)} pos={venue_positions})"
            )

    text = (
        f"independent demo REST: open_orders={len(order_rows)} positions={venue_positions}"
    )
    if args.grpc_status:
        text += (
            f" grpc orders={snapshot.get('grpc_orders')} "
            f"positions={snapshot.get('grpc_positions')} "
            f"strategy_pos={snapshot.get('grpc_strategy_pos')}"
        )
    print(text)
    if args.out:
        args.out.write_text(json.dumps(snapshot, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
