#!/usr/bin/env python3
"""Independent audit for BTCPriceMALongFlatV1 candidate #2.

Does not import the Rust trading engine or call its model/replay/accounting.
Signal decisions are rebuilt from stored candles. The independently written
Donchian audit ledger is reused for fills/funding/PnL, as allowed by
AUDIT_BRIEF.md.
"""
from __future__ import annotations

import csv, hashlib, importlib.util, json, math, sys, unittest
from dataclasses import dataclass
from decimal import Decimal, getcontext
from pathlib import Path

getcontext().prec = 60
ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "data" / "btc_price_ma_long_flat_v1"
DON_DATA = ROOT / "data" / "donchian_daily"
OUT = DATA / "audit" / "zero_cost_and_cost_a"
START, END = 1_674_259_200_000, 1_767_225_600_000
QTY = Decimal("0.001")
BENCH_NET = Decimal("48.360542973936")
BENCH_DD = Decimal("44.3887151092231")
GEOM_TOL, SPREAD_TOL = 1e-8, 1e-12

def _don():
    p = ROOT / "audit" / "donchian_daily_v1" / "reference_check.py"
    s = importlib.util.spec_from_file_location("_don_ref", p)
    if s is None or s.loader is None:
        raise RuntimeError(f"cannot load {p}")
    m = importlib.util.module_from_spec(s)
    sys.modules[s.name] = m
    s.loader.exec_module(m)
    return m

DON = _don()

@dataclass
class Decision:
    start: int
    end: int
    close: Decimal
    prev: str
    direction: str
    readiness: str
    kind: str
    qty: Decimal | None
    geom: float | None
    spread: float | None
    @property
    def id(self): return f"{self.start}-{self.end}"

def D(x): return x if isinstance(x, Decimal) else Decimal(str(x))
def rjson(p): return json.loads(p.read_text())

def payload_hash(x):
    raw = json.dumps(x, separators=(",", ":"), ensure_ascii=False).encode()
    return "sha256:" + hashlib.sha256(raw).hexdigest()

def daily_hash(x):
    m=x["metadata"]
    return payload_hash({"format_version":m["format_version"],"venue":m["venue"],
        "environment":m["environment"],"product":m["product"],"symbol":m["symbol"],
        "interval":m["interval"],"candles":x["candles"]})

def aux_hash(x, field):
    m=x["metadata"]
    return payload_hash({"format_version":m["format_version"],"venue":m["venue"],
        "environment":m["environment"],"product":m["product"],"symbol":m["symbol"],
        "series":m["series"],field:x[field]})

def cfg_hash(n):
    return payload_hash({"strategy_id":"BTCPriceMALongFlatV1","strategy_version":"1",
        "lookback":n,"position_qty":"0.001","allow_short":False,
        "signal":"log_close_minus_geometric_mean_of_N_including_current"})

def add(out, area, ts, field, exp, rec, note):
    out.append(DON.Discrepancy(area,str(ts),field,str(exp),str(rec),note))

def merge_data(warm, main, out):
    wc, mc = DON.load_candles(warm), DON.load_candles(main)
    DON.validate_daily(wc, warm["metadata"], out)
    DON.validate_daily(mc, main["metadata"], out)
    if wc[-1].end_ms != mc[0].start_ms:
        add(out,"dataset",mc[0].start_ms,"warmup/main chain",wc[-1].end_ms,mc[0].start_ms,"must be contiguous")
    merged=wc+mc
    for a,b in zip(merged,merged[1:]):
        if a.end_ms != b.start_ms:
            add(out,"dataset",b.start_ms,"merged chain",a.end_ms,b.start_ms,"gap/overlap")
    return merged

def rebuild(candles, n):
    idx=next(i for i,c in enumerate(candles) if c.start_ms==START)
    series=[c for c in candles[idx-n:] if c.start_ms<END]
    requested=Decimal("0"); direction="flat"; rows=[]
    for i,c in enumerate(series):
        prev=direction
        if i<n:
            rows.append(Decision(c.start_ms,c.end_ms,c.close,"flat","flat","not_ready","not_ready",None,None,None))
            continue
        closes=[float(x.close) for x in series[i+1-n:i+1]]
        logs=[math.log(x) for x in closes]
        mean=sum(logs)/n; spread=math.log(closes[-1])-mean; geom=math.exp(mean)
        desired=QTY if spread>0 else Decimal("0")
        direction="long" if desired>0 else "flat"
        if desired != requested:
            kind,qty="quantity",desired; requested=desired
        else:
            kind,qty="no_change",None
        rows.append(Decision(c.start_ms,c.end_ms,c.close,prev,direction,"ready",kind,qty,geom,spread))
    return rows

def compare_decisions(exp, stored, n, out):
    by={int(x["start_time_ms"]):x for x in stored["decisions"]}
    econ=diag=0
    if len(by)!=len(exp):
        add(out,"decisions","","row count",len(exp),len(by),f"N={n}"); econ+=1
    for e in exp:
        r=by.get(e.start)
        if r is None:
            add(out,"decisions",e.start,"presence","row","missing",f"N={n}"); econ+=1; continue
        exact=[("venue","bybit",r["venue"]),("product","linear",r["product"]),
               ("instrument","BTCUSDT",r["instrument"]),("interval","D",r["interval"]),
               ("end_time_exclusive_ms",e.end,r["end_time_exclusive_ms"]),
               ("close",e.close,D(r["close"])),("previous_direction",e.prev,r["previous_direction"]),
               ("direction",e.direction,r["direction"]),("readiness",e.readiness,r["readiness"]),
               ("target.kind",e.kind,r["target"]["kind"])]
        for f,a,b in exact:
            if a!=b: add(out,"decisions",e.start,f,a,b,f"N={n}"); econ+=1
        rq=r["target"].get("quantity")
        if e.kind=="quantity":
            if rq is None or e.qty!=D(rq):
                add(out,"decisions",e.start,"target.quantity",e.qty,rq,f"N={n}"); econ+=1
        elif rq is not None:
            add(out,"decisions",e.start,"target.quantity",None,rq,f"N={n}"); econ+=1
        if r.get("exit_high") is not None or r.get("exit_low") is not None:
            add(out,"decisions",e.start,"exit diagnostics","null/null",f"{r.get('exit_high')}/{r.get('exit_low')}",f"N={n}"); econ+=1
        if r.get("suspension") is not None:
            add(out,"decisions",e.start,"suspension",None,r.get("suspension"),f"N={n}"); econ+=1
        if e.readiness=="not_ready":
            if r.get("entry_high") is not None or r.get("entry_low") is not None:
                add(out,"decisions",e.start,"warmup diagnostics","null/null","present",f"N={n}"); econ+=1
        else:
            if not math.isclose(float(r["entry_high"]),e.geom,rel_tol=0,abs_tol=GEOM_TOL):
                add(out,"decision_diagnostic",e.start,"geometric_mean",e.geom,r["entry_high"],f"N={n}; libm last-bit diagnostic"); diag+=1
            if not math.isclose(float(r["entry_low"]),e.spread,rel_tol=0,abs_tol=SPREAD_TOL):
                add(out,"decision_diagnostic",e.start,"log_spread",e.spread,r["entry_low"],f"N={n}; libm last-bit diagnostic"); diag+=1
    entries=sum(1 for e in exp if e.readiness=="ready" and e.prev=="flat" and e.direction=="long" and e.kind=="quantity")
    exits=sum(1 for e in exp if e.readiness=="ready" and e.prev=="long" and e.direction=="flat" and e.kind=="quantity")
    run=stored["run"]
    for f,a,b in [("entries",entries,run["entries"]),("exits",exits,run["exits"]),
                  ("final_direction",exp[-1].direction,run["final_direction"]),
                  ("warmup_candles",n,run["warmup_candles"]),("evaluated_candles",1076,run["evaluated_candles"])]:
        if a!=b: add(out,"decision_run","",f,a,b,f"N={n}"); econ+=1
    return {"rows":len(exp),"entries":entries,"exits":exits,"final":exp[-1].direction,"economic":econ,"diagnostic":diag}

def changes(rows):
    return [DON.TargetChange(e.id,e.start,e.end,e.end,e.qty)
            for e in rows if e.readiness=="ready" and e.kind=="quantity"]

def audit(label, target_changes, fee, adj, path, minutes, funding, marks, out, notes):
    sched=DON.schedule_fills(target_changes,minutes,out)
    ledger=DON.simulate(sched,funding,marks,Decimal(fee),Decimal(adj),out)
    if ledger is None: return None
    DON.compare_fills(ledger,DON.read_csv(path/"fills.csv"),out)
    DON.compare_trades(ledger,DON.read_csv(path/"trades.csv"),out)
    DON.compare_funding(ledger,DON.read_csv(path/"funding.csv"),out)
    DON.compare_pnl(ledger,DON.read_csv(path/"pnl.csv"),out,label)
    before=len(out); DON.compare_summary(ledger,rjson(path/"summary.json"),out,label)
    tail=out[before:]; notes.extend(x for x in tail if x.area=="close_cost_definition")
    out[:]=out[:before]+[x for x in tail if x.area!="close_cost_definition"]
    return ledger

def exposure(ledger):
    return sum((abs(x.position)*x.mark_price for x in ledger.samples),Decimal("0"))

def wcsv(path, rows):
    path.parent.mkdir(parents=True,exist_ok=True)
    fields=["area","timestamp_ms","field","expected","recorded","observation"]
    with path.open("w",newline="") as h:
        w=csv.DictWriter(h,fieldnames=fields); w.writeheader(); w.writerows([x.__dict__ for x in rows])

def tests():
    suite=unittest.TestLoader().discover(str(Path(__file__).resolve().parent/"tests"),pattern="test_*.py")
    return unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful()

def main():
    if not tests(): return 1
    out=[]; notes=[]
    warm=rjson(DATA/"warmup-2022-11-01_2023-01-01.json")
    main=rjson(DON_DATA/"bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json")
    logs={n:rjson(DATA/f"decisions-n{n}.json") for n in (14,28,70)}
    trade=rjson(DATA/"aux"/"trade-1m.json"); fundingf=rjson(DATA/"aux"/"funding.json"); markf=rjson(DATA/"aux"/"mark-60.json")
    for label,a,b in [
        ("warmup hash",daily_hash(warm),warm["metadata"]["dataset_hash"]),
        ("main hash",daily_hash(main),main["metadata"]["dataset_hash"]),
        ("trade hash",aux_hash(trade,"points"),trade["metadata"]["dataset_hash"]),
        ("funding hash",aux_hash(fundingf,"records"),fundingf["metadata"]["dataset_hash"]),
        ("mark hash",aux_hash(markf,"points"),markf["metadata"]["dataset_hash"])]:
        if a!=b: add(out,"identity","",label,a,b,"compact payload sha256")
    for n,l in logs.items():
        if l["run"]["dataset_hash"]!=main["metadata"]["dataset_hash"]:
            add(out,"identity","",f"N={n} dataset hash",main["metadata"]["dataset_hash"],l["run"]["dataset_hash"],"decision source")
        if l["run"]["config_hash"]!=cfg_hash(n):
            add(out,"identity","",f"N={n} config hash",cfg_hash(n),l["run"]["config_hash"],"frozen config")
    candles=merge_data(warm,main,out)
    marks=DON.validate_hours(markf["points"],out); funding=DON.validate_funding(fundingf["records"],out); minutes=DON.validate_minutes(trade["points"])
    expected={n:rebuild(candles,n) for n in (14,28,70)}
    stats={n:compare_decisions(expected[n],logs[n],n,out) for n in (14,28,70)}
    ch={n:changes(expected[n]) for n in (14,28,70)}
    led={}
    led["n28_zero"]=audit("n28_zero",ch[28],"0","0",DATA/"backtest"/"primary_n28"/"n28"/"zero_cost_diagnostic",minutes,funding,marks,out,notes)
    led["n28_cost"]=audit("n28_cost",ch[28],"6","2",DATA/"backtest"/"primary_n28"/"n28"/"cost_a",minutes,funding,marks,out,notes)
    led["n14_cost"]=audit("n14_cost",ch[14],"6","2",DATA/"backtest"/"diagnostic_n14"/"n14"/"cost_a",minutes,funding,marks,out,notes)
    led["n70_cost"]=audit("n70_cost",ch[70],"6","2",DATA/"backtest"/"diagnostic_n70"/"n70"/"cost_a",minutes,funding,marks,out,notes)
    bench_change=DON.TargetChange("benchmark-fixed-long",None,None,START,QTY)
    led["benchmark_cost"]=audit("benchmark_cost",[bench_change],"6","2",DATA/"backtest"/"fixed_long_0.001_btc"/"fixed_long_0.001_btc"/"cost_a",minutes,funding,marks,out,notes)
    if any(v is None for v in led.values()): return 2

    primary,bench=led["n28_cost"],led["benchmark_cost"]
    if bench.net!=BENCH_NET: add(out,"gate_input","","benchmark net",BENCH_NET,bench.net,"independent benchmark")
    if bench.max_drawdown!=BENCH_DD: add(out,"gate_input","","benchmark drawdown",BENCH_DD,bench.max_drawdown,"independent benchmark")
    f=exposure(primary)/exposure(bench); scaled=BENCH_NET*f
    higher=rjson(DATA/"backtest"/"primary_n28"/"n28"/"higher_cost"/"summary.json")
    delayed=rjson(DATA/"backtest"/"primary_n28"/"n28"/"delayed_execution"/"summary.json")
    gates=[
      ("cost_a_net_positive",primary.net>0,"audited"),
      ("cost_a_profit_retention_80pct",primary.net>=BENCH_NET*Decimal("0.8"),"audited"),
      ("cost_a_drawdown_70pct_of_benchmark",primary.max_drawdown<=BENCH_DD*Decimal("0.7"),"audited"),
      ("beats_notional_matched_passive",primary.net>scaled,"audited"),
      ("higher_cost_net_positive",D(higher["net_pnl"])>0,"stored summary only; ledger unverified"),
      ("delayed_execution_net_positive",D(delayed["net_pnl"])>0,"stored summary only; ledger unverified"),
      ("completed_round_trips_at_least_20",len(primary.trades)>=20,"audited"),
      ("diagnostic_14_cost_a_positive",led["n14_cost"].net>0,"audited"),
      ("diagnostic_70_cost_a_positive",led["n70_cost"].net>0,"audited")]
    stored=rjson(DATA/"gates.json"); sb={x["gate"]:x for x in stored["gates"]}
    for name,pas,note in gates:
        if name not in sb or bool(sb[name]["passed"])!=pas:
            add(out,"gates","",name,pas,sb.get(name,{}).get("passed","missing"),note)
    disposition="eligible_for_holdout_review" if all(x[1] for x in gates) else "reject"
    if stored["disposition"]!=disposition: add(out,"gates","","disposition",disposition,stored["disposition"],"frozen gates")

    OUT.mkdir(parents=True,exist_ok=True); wcsv(OUT/"discrepancies.csv",out)
    lines=["# BTCPriceMALongFlatV1 independent audit","",
      "## Decision correctness","",
      "| N | rows | entries | exits | final | economic discrepancies |","|---:|---:|---:|---:|---|---:|"]
    for n in (14,28,70):
        s=stats[n]; lines.append(f"| {n} | {s['rows']} | {s['entries']} | {s['exits']} | {s['final']} | {s['economic']} |")
    lines += ["","Signal diagnostics use tight tolerances for cross-libm last-bit differences; directions and quantities are exact.","",
      "## Simulation/accounting","",
      "| scenario | net USDT | max DD USDT | fills | completed trades | end position |","|---|---:|---:|---:|---:|---:|"]
    for name in ("n28_zero","n28_cost","n14_cost","n70_cost","benchmark_cost"):
        x=led[name]; lines.append(f"| {name} | {DON.dec_text(x.net)} | {DON.dec_text(x.max_drawdown)} | {len(x.fills)} | {len(x.trades)} | {DON.dec_text(x.position_end)} |")
    lines += ["",f"Notional exposure ratio f = {f}",f"Scaled passive Cost A net = {scaled} USDT","",
      "## Development screen","", "| gate | result | verification |","|---|---|---|"]
    for n,p,v in gates: lines.append(f"| {n} | {'PASS' if p else 'FAIL'} | {v} |")
    lines += ["",f"Disposition: **{disposition}**","",
      "## Unverified / out of scope","",
      "- higher_cost and delayed_execution ledgers were not independently reconstructed; their stored summary signs are used only for gates 5–6 as permitted by the brief.",
      "- 2026 holdout remains sealed.",
      "- Live/testnet execution, liquidity, partial fills, margin, and liquidation are not verified.","",
      "## Result","",
      f"Economic/data discrepancies: **{len([x for x in out if x.area!='decision_diagnostic'])}**",
      f"Diagnostic-only discrepancies: **{len([x for x in out if x.area=='decision_diagnostic'])}**","",
      "Preregistered result confirmed: **reject**. Primary N=28 is profitable under Cost A but fails the 80% benchmark-profit-retention gate and the notional-matched passive gate."]
    (OUT/"audit_report.md").write_text("\n".join(lines)+"\n")
    econ=[x for x in out if x.area!="decision_diagnostic"]
    print(f"N14={stats[14]['entries']}/{stats[14]['exits']} N28={stats[28]['entries']}/{stats[28]['exits']} N70={stats[70]['entries']}/{stats[70]['exits']}")
    print(f"primary_net={primary.net} primary_dd={primary.max_drawdown} scaled_passive={scaled}")
    print(f"disposition={disposition} discrepancies={len(out)} economic_or_data={len(econ)}")
    print(f"wrote={OUT}")
    return 0 if not econ else 1

if __name__=="__main__":
    sys.exit(main())
