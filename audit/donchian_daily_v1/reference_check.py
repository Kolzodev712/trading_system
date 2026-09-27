#!/usr/bin/env python3
"""Independent reference check for the frozen DonchianDailyV1 backtest.

This file does not import the trading engine and does not call its model,
replay, fill, funding, or accounting functions. Decisions are rebuilt from
the stored daily candles. Fills, funding, and PnL are rebuilt from those
decisions plus the stored one-minute opens, funding rates, and hourly marks.

Reproduce from the repository root:

    python3 audit/donchian_daily_v1/reference_check.py

Audited scenarios: zero-trading-cost diagnostic, and Cost A.
The higher-cost and delayed-execution scenarios are not audited.
"""

from __future__ import annotations

import csv
import hashlib
import json
from dataclasses import dataclass, field
from datetime import datetime, timezone
from decimal import Decimal, getcontext
from pathlib import Path

getcontext().prec = 60

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "data" / "donchian_daily"
OUT = DATA / "audit" / "zero_cost_and_cost_a"

DAY_MS = 86_400_000
HOUR_MS = 3_600_000
MINUTE_MS = 60_000
DELAY_MS = 10_000
ENTRY_LOOKBACK = 20
EXIT_LOOKBACK = 10
QTY = Decimal("0.001")
PERFORMANCE_START_MS = 1_674_259_200_000
PERFORMANCE_END_MS = 1_767_225_600_000
YEAR_2024_MS = 1_704_067_200_000
YEAR_2025_MS = 1_735_689_600_000

ZERO = Decimal("0")
ONE = Decimal("1")
BPS = Decimal("10000")


def D(value) -> Decimal:
    if isinstance(value, Decimal):
        return value
    return Decimal(str(value))


def iso(ms: int) -> str:
    return datetime.fromtimestamp(ms / 1000, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def dec_text(value: Decimal) -> str:
    text = format(value, "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    return text or "0"


def same(left, right) -> bool:
    return D(left) == D(right)


def values_match(expected, recorded) -> bool:
    if expected is None and recorded is None:
        return True
    try:
        return D(expected) == D(recorded)
    except Exception:
        return str(expected) == str(recorded)


def sha256_payload(payload: dict) -> str:
    raw = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode()
    return "sha256:" + hashlib.sha256(raw).hexdigest()


def read_json(path: Path) -> dict:
    return json.loads(path.read_text())


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="") as handle:
        return list(csv.DictReader(handle))


def write_csv(path: Path, rows: list[dict], fieldnames: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


@dataclass
class Candle:
    start_ms: int
    end_ms: int
    open: Decimal
    high: Decimal
    low: Decimal
    close: Decimal
    volume: Decimal


@dataclass
class ExpectedDecision:
    start_ms: int
    end_ms: int
    close: Decimal
    entry_high: Decimal | None
    entry_low: Decimal | None
    exit_high: Decimal | None
    exit_low: Decimal | None
    entry_window_start_ms: int | None
    entry_window_end_ms: int | None
    exit_window_start_ms: int | None
    exit_window_end_ms: int | None
    entry_high_at_ms: str
    entry_low_at_ms: str
    exit_high_at_ms: str
    exit_low_at_ms: str
    previous_direction: str
    direction: str
    readiness: str
    reason: str
    target_kind: str
    target_qty: Decimal | None

    @property
    def decision_id(self) -> str:
        return f"{self.start_ms}-{self.end_ms}"


@dataclass
class TargetChange:
    decision_id: str
    source_start_ms: int | None
    source_end_ms: int | None
    decision_time_ms: int
    target_qty: Decimal


@dataclass
class ScheduledFill:
    change: TargetChange
    available_at_ms: int
    fill_time_ms: int
    reference_open: Decimal
    pending: bool
    pending_reason: str


@dataclass
class Trade:
    entry_decision_id: str
    exit_decision_id: str
    side: str
    quantity: Decimal
    entry_time_ms: int
    exit_time_ms: int
    entry_fill: Decimal
    exit_fill: Decimal
    price_pnl: Decimal
    fees: Decimal
    funding: Decimal

    @property
    def net_pnl(self) -> Decimal:
        return self.price_pnl - self.fees + self.funding

    @property
    def holding_ms(self) -> int:
        return self.exit_time_ms - self.entry_time_ms


@dataclass
class FillRow:
    decision_id: str
    source_start_ms: int | None
    source_end_ms: int | None
    decision_time_ms: int
    available_at_ms: int
    fill_time_ms: int
    side: str
    quantity: Decimal
    reference_open: Decimal
    fill_price: Decimal
    fee: Decimal
    position_before: Decimal
    position_after: Decimal


@dataclass
class FundingFlow:
    settlement_time_ms: int
    signed_position: Decimal
    funding_rate: Decimal
    mark_price: Decimal
    mark_time_ms: int
    mark_method: str
    cashflow: Decimal


@dataclass
class Sample:
    observation_time_ms: int
    mark_time_ms: int
    mark_price: Decimal
    mark_source: str
    position: Decimal
    realized: Decimal
    unrealized: Decimal
    fees: Decimal
    funding: Decimal
    net: Decimal
    running_peak: Decimal
    drawdown: Decimal


@dataclass
class Ledger:
    fills: list[FillRow]
    pending: list[ScheduledFill]
    trades: list[Trade]
    funding: list[FundingFlow]
    samples: list[Sample]
    position_end: Decimal
    realized: Decimal
    unrealized: Decimal
    fees: Decimal
    funding_total: Decimal
    net: Decimal
    gross: Decimal
    funding_paid: Decimal
    funding_received: Decimal
    long_net: Decimal
    short_net: Decimal
    open_mark_to_market: Decimal
    max_drawdown: Decimal
    max_drawdown_time_ms: int
    max_drawdown_peak_time_ms: int
    time_in_market_ms: int
    max_notional: Decimal
    years: list[dict]
    estimated_close_cost_on_mark: Decimal
    estimated_close_cost_on_adjusted_fill: Decimal


@dataclass
class Discrepancy:
    area: str
    timestamp_ms: str
    field: str
    expected: str
    recorded: str
    observation: str


@dataclass
class ScenarioSpec:
    name: str
    fee_bps: Decimal
    adjustment_bps: Decimal
    book: str


def load_candles(dataset: dict) -> list[Candle]:
    candles = []
    for row in dataset["candles"]:
        candles.append(
            Candle(
                start_ms=int(row["start_time_ms"]),
                end_ms=int(row["end_time_exclusive_ms"]),
                open=D(row["open"]),
                high=D(row["high"]),
                low=D(row["low"]),
                close=D(row["close"]),
                volume=D(row["volume"]),
            )
        )
    return candles


def times_at_high(window: list[Candle]) -> str:
    level = max(candle.high for candle in window)
    return ",".join(str(candle.start_ms) for candle in window if candle.high == level)


def times_at_low(window: list[Candle]) -> str:
    level = min(candle.low for candle in window)
    return ",".join(str(candle.start_ms) for candle in window if candle.low == level)


def reconstruct_decisions(candles: list[Candle]) -> list[ExpectedDecision]:
    """20/10 Donchian on completed daily candles.

    The current candle is not in either channel. It is appended only after
    the decision. Warm-up is the 20 candles immediately before the evaluation
    start. Earlier candles are not part of the model history. A flat model
    enters long only when close is strictly above the prior 20-candle high,
    and short only when close is strictly below the prior 20-candle low.
    A long exits only when close is strictly below the prior 10-candle low.
    A short exits only when close is strictly above the prior 10-candle high.
    An exit returns to flat on that candle; it does not reverse.
    """
    eval_index = next(
        (index for index, candle in enumerate(candles) if candle.start_ms == PERFORMANCE_START_MS),
        None,
    )
    if eval_index is None:
        raise SystemExit("evaluation start candle is not in the daily dataset")
    if eval_index < ENTRY_LOOKBACK:
        raise SystemExit(
            f"only {eval_index} candles precede the evaluation start; 20 warm-up candles are required"
        )
    model_candles = candles[eval_index - ENTRY_LOOKBACK :]
    model_candles = [candle for candle in model_candles if candle.start_ms < PERFORMANCE_END_MS]
    history: list[Candle] = []
    direction = "flat"
    decisions: list[ExpectedDecision] = []
    for candle in model_candles:
        previous = direction
        entry_high = entry_low = exit_high = exit_low = None
        entry_start = entry_end = exit_start = exit_end = None
        entry_high_at = entry_low_at = exit_high_at = exit_low_at = ""
        if len(history) < ENTRY_LOOKBACK:
            decisions.append(
                ExpectedDecision(
                    start_ms=candle.start_ms,
                    end_ms=candle.end_ms,
                    close=candle.close,
                    entry_high=None,
                    entry_low=None,
                    exit_high=None,
                    exit_low=None,
                    entry_window_start_ms=None,
                    entry_window_end_ms=None,
                    exit_window_start_ms=None,
                    exit_window_end_ms=None,
                    entry_high_at_ms="",
                    entry_low_at_ms="",
                    exit_high_at_ms="",
                    exit_low_at_ms="",
                    previous_direction=previous,
                    direction="flat",
                    readiness="not_ready",
                    reason=f"not ready: {len(history)} of {ENTRY_LOOKBACK} preceding candles",
                    target_kind="not_ready",
                    target_qty=None,
                )
            )
            history.append(candle)
            continue
        entry_window = history[-ENTRY_LOOKBACK:]
        exit_window = history[-EXIT_LOOKBACK:]
        entry_high = max(item.high for item in entry_window)
        entry_low = min(item.low for item in entry_window)
        exit_high = max(item.high for item in exit_window)
        exit_low = min(item.low for item in exit_window)
        entry_start = entry_window[0].start_ms
        entry_end = entry_window[-1].end_ms
        exit_start = exit_window[0].start_ms
        exit_end = exit_window[-1].end_ms
        entry_high_at = times_at_high(entry_window)
        entry_low_at = times_at_low(entry_window)
        exit_high_at = times_at_high(exit_window)
        exit_low_at = times_at_low(exit_window)
        target_kind = "no_change"
        target_qty = None
        if direction == "flat":
            if candle.close > entry_high:
                direction = "long"
                target_kind = "quantity"
                target_qty = QTY
                reason = "close above entry high"
            elif candle.close < entry_low:
                direction = "short"
                target_kind = "quantity"
                target_qty = -QTY
                reason = "close below entry low"
            else:
                reason = "no entry"
        elif direction == "long":
            if candle.close < exit_low:
                direction = "flat"
                target_kind = "quantity"
                target_qty = ZERO
                reason = "close below exit low"
            else:
                reason = "long retained"
        else:
            if candle.close > exit_high:
                direction = "flat"
                target_kind = "quantity"
                target_qty = ZERO
                reason = "close above exit high"
            else:
                reason = "short retained"
        decisions.append(
            ExpectedDecision(
                start_ms=candle.start_ms,
                end_ms=candle.end_ms,
                close=candle.close,
                entry_high=entry_high,
                entry_low=entry_low,
                exit_high=exit_high,
                exit_low=exit_low,
                entry_window_start_ms=entry_start,
                entry_window_end_ms=entry_end,
                exit_window_start_ms=exit_start,
                exit_window_end_ms=exit_end,
                entry_high_at_ms=entry_high_at,
                entry_low_at_ms=entry_low_at,
                exit_high_at_ms=exit_high_at,
                exit_low_at_ms=exit_low_at,
                previous_direction=previous,
                direction=direction,
                readiness="ready",
                reason=reason,
                target_kind=target_kind,
                target_qty=target_qty,
            )
        )
        history.append(candle)
    return decisions


def target_changes(decisions: list[ExpectedDecision]) -> list[TargetChange]:
    changes = []
    for decision in decisions:
        if decision.target_kind != "quantity":
            continue
        changes.append(
            TargetChange(
                decision_id=decision.decision_id,
                source_start_ms=decision.start_ms,
                source_end_ms=decision.end_ms,
                decision_time_ms=decision.end_ms,
                target_qty=decision.target_qty if decision.target_qty is not None else ZERO,
            )
        )
    return changes


def next_minute_strictly_after(available_at_ms: int) -> int:
    if available_at_ms % MINUTE_MS == 0:
        return available_at_ms + MINUTE_MS
    boundary = available_at_ms - (available_at_ms % MINUTE_MS)
    return boundary + MINUTE_MS


def schedule_fills(
    changes: list[TargetChange],
    minutes: dict[int, Decimal],
    missing: list[Discrepancy],
) -> list[ScheduledFill]:
    scheduled = []
    for change in changes:
        available = change.decision_time_ms + DELAY_MS
        fill_time = next_minute_strictly_after(available)
        outside = (
            change.decision_time_ms >= PERFORMANCE_END_MS
            or available >= PERFORMANCE_END_MS
            or fill_time >= PERFORMANCE_END_MS
        )
        if outside:
            scheduled.append(
                ScheduledFill(
                    change=change,
                    available_at_ms=available,
                    fill_time_ms=fill_time,
                    reference_open=ZERO,
                    pending=True,
                    pending_reason=(
                        "available_at or the next one-minute open is outside the performance window"
                    ),
                )
            )
            continue
        if fill_time not in minutes:
            missing.append(
                Discrepancy(
                    area="execution_evidence",
                    timestamp_ms=str(fill_time),
                    field="trade_1m_open",
                    expected="stored one-minute open",
                    recorded="missing",
                    observation=(
                        f"decision {change.decision_id} requires the trade candle "
                        f"starting {iso(fill_time)}; the sparse tape has no such row"
                    ),
                )
            )
            scheduled.append(
                ScheduledFill(
                    change=change,
                    available_at_ms=available,
                    fill_time_ms=fill_time,
                    reference_open=ZERO,
                    pending=False,
                    pending_reason="missing execution observation",
                )
            )
            continue
        scheduled.append(
            ScheduledFill(
                change=change,
                available_at_ms=available,
                fill_time_ms=fill_time,
                reference_open=minutes[fill_time],
                pending=False,
                pending_reason="",
            )
        )
    return scheduled


def adjusted_fill(reference: Decimal, buy: bool, adjustment_bps: Decimal) -> Decimal:
    factor = ONE + (adjustment_bps / BPS) if buy else ONE - (adjustment_bps / BPS)
    return reference * factor


def close_cost_on_mark(position: Decimal, mark: Decimal, fee_bps: Decimal, adjustment_bps: Decimal) -> Decimal:
    if position == ZERO:
        return ZERO
    return abs(position) * mark * (fee_bps + adjustment_bps) / BPS


def close_cost_on_adjusted_fill(
    position: Decimal, mark: Decimal, fee_bps: Decimal, adjustment_bps: Decimal
) -> Decimal:
    """Close a long by selling, or a short by buying, at an adverse price.

    The disclosed cost is the gap from the mark to that fill, plus a fee on
    the adjusted fill. This is not subtracted from net PnL.
    """
    if position == ZERO:
        return ZERO
    buy_to_close = position < ZERO
    fill = adjusted_fill(mark, buy_to_close, adjustment_bps)
    gap = abs(position * (fill - mark))
    fee = abs(position) * fill * fee_bps / BPS
    return gap + fee


@dataclass
class OpenTrade:
    entry_decision_id: str
    side: str
    quantity: Decimal
    entry_time_ms: int
    entry_fill: Decimal
    entry_fee: Decimal
    funding: Decimal = field(default_factory=lambda: ZERO)


def settlement_mark(marks: dict[int, Decimal], timestamp_ms: int, missing: list[Discrepancy]) -> tuple[Decimal, int, str] | None:
    if timestamp_ms % HOUR_MS != 0 or timestamp_ms not in marks:
        missing.append(
            Discrepancy(
                area="funding_mark",
                timestamp_ms=str(timestamp_ms),
                field="hourly_mark_open",
                expected="open of the hourly mark candle at the settlement timestamp",
                recorded="missing or not on an hour",
                observation="no finer mark file is consulted; a missing hour is missing evidence",
            )
        )
        return None
    return marks[timestamp_ms], timestamp_ms, "hourly_mark_open_at_settlement"


def mark_at_or_before(marks: dict[int, Decimal], timestamp_ms: int) -> tuple[int, Decimal]:
    hour = timestamp_ms - (timestamp_ms % HOUR_MS)
    if hour not in marks:
        raise SystemExit(f"no hourly mark at or before {timestamp_ms}")
    return hour, marks[hour]


def simulate(
    scheduled: list[ScheduledFill],
    funding_rows: list[tuple[int, Decimal]],
    marks: dict[int, Decimal],
    fee_bps: Decimal,
    adjustment_bps: Decimal,
    missing: list[Discrepancy],
) -> Ledger | None:
    if any(item.pending_reason == "missing execution observation" for item in scheduled):
        return None
    if any(item.area == "funding_mark" for item in missing):
        return None
    actionable = [item for item in scheduled if not item.pending]
    pending = [item for item in scheduled if item.pending]
    fill_by_time: dict[int, ScheduledFill] = {}
    for item in actionable:
        if item.fill_time_ms in fill_by_time:
            missing.append(
                Discrepancy(
                    area="execution",
                    timestamp_ms=str(item.fill_time_ms),
                    field="fill_collision",
                    expected="one fill at this minute",
                    recorded="two reconstructed fills",
                    observation="refusing to invent an order between overlapping fills",
                )
            )
            return None
        fill_by_time[item.fill_time_ms] = item
    in_window_funding = [
        (timestamp, rate)
        for timestamp, rate in funding_rows
        if PERFORMANCE_START_MS <= timestamp < PERFORMANCE_END_MS
    ]
    times = set(range(PERFORMANCE_START_MS, PERFORMANCE_END_MS + HOUR_MS, HOUR_MS))
    times.update(timestamp for timestamp, _rate in in_window_funding)
    times.update(fill_by_time)
    position = ZERO
    realized = ZERO
    fees = ZERO
    funding_total = ZERO
    open_trade: OpenTrade | None = None
    fills: list[FillRow] = []
    trades: list[Trade] = []
    flows: list[FundingFlow] = []
    samples: list[Sample] = []
    peak = ZERO
    peak_time = PERFORMANCE_START_MS
    max_drawdown = ZERO
    max_drawdown_time = PERFORMANCE_START_MS
    max_drawdown_peak_time = PERFORMANCE_START_MS
    max_notional = ZERO
    opened_at: int | None = None
    held_ms = 0

    def observe(time_ms: int, mark_time: int, mark_price: Decimal, source: str) -> None:
        nonlocal peak, peak_time, max_drawdown, max_drawdown_time, max_drawdown_peak_time, max_notional
        unrealized = position * (mark_price - open_trade.entry_fill) if open_trade else ZERO
        net = realized + unrealized - fees + funding_total
        if net > peak:
            peak = net
            peak_time = time_ms
        drawdown = peak - net
        if drawdown > max_drawdown:
            max_drawdown = drawdown
            max_drawdown_time = time_ms
            max_drawdown_peak_time = peak_time
        if position != ZERO:
            notional = abs(position) * mark_price
            if notional > max_notional:
                max_notional = notional
        samples.append(
            Sample(
                observation_time_ms=time_ms,
                mark_time_ms=mark_time,
                mark_price=mark_price,
                mark_source=source,
                position=position,
                realized=realized,
                unrealized=unrealized,
                fees=fees,
                funding=funding_total,
                net=net,
                running_peak=peak,
                drawdown=drawdown,
            )
        )

    funding_by_time = {timestamp: rate for timestamp, rate in in_window_funding}
    for time_ms in sorted(times):
        sampled = False
        if time_ms in funding_by_time:
            marked = settlement_mark(marks, time_ms, missing)
            if marked is None:
                return None
            mark_price, mark_time, method = marked
            cashflow = -position * mark_price * funding_by_time[time_ms]
            if open_trade is not None:
                open_trade.funding += cashflow
            funding_total += cashflow
            flows.append(
                FundingFlow(
                    settlement_time_ms=time_ms,
                    signed_position=position,
                    funding_rate=funding_by_time[time_ms],
                    mark_price=mark_price,
                    mark_time_ms=mark_time,
                    mark_method=method,
                    cashflow=cashflow,
                )
            )
            observe(time_ms, mark_time, mark_price, method)
            sampled = True
        if time_ms in fill_by_time:
            item = fill_by_time[time_ms]
            before = position
            delta = item.change.target_qty - before
            if delta == ZERO or (before != ZERO and before + delta != ZERO):
                missing.append(
                    Discrepancy(
                        area="execution",
                        timestamp_ms=str(time_ms),
                        field="position_transition",
                        expected="full open from flat or full close to flat",
                        recorded=f"before {dec_text(before)} target {dec_text(item.change.target_qty)}",
                        observation="this checker does not model adds, reductions, or same-candle reversals",
                    )
                )
                return None
            buy = delta > ZERO
            fill_price = adjusted_fill(item.reference_open, buy, adjustment_bps)
            fee = abs(delta) * fill_price * fee_bps / BPS
            fees += fee
            after = before + delta
            fill_notional = abs(after) * fill_price
            if fill_notional > max_notional:
                max_notional = fill_notional
            fills.append(
                FillRow(
                    decision_id=item.change.decision_id,
                    source_start_ms=item.change.source_start_ms,
                    source_end_ms=item.change.source_end_ms,
                    decision_time_ms=item.change.decision_time_ms,
                    available_at_ms=item.available_at_ms,
                    fill_time_ms=item.fill_time_ms,
                    side="buy" if buy else "sell",
                    quantity=delta,
                    reference_open=item.reference_open,
                    fill_price=fill_price,
                    fee=fee,
                    position_before=before,
                    position_after=after,
                )
            )
            if before == ZERO:
                open_trade = OpenTrade(
                    entry_decision_id=item.change.decision_id,
                    side="long" if after > ZERO else "short",
                    quantity=after,
                    entry_time_ms=item.fill_time_ms,
                    entry_fill=fill_price,
                    entry_fee=fee,
                )
                position = after
                opened_at = item.fill_time_ms
            else:
                assert open_trade is not None
                price_pnl = open_trade.quantity * (fill_price - open_trade.entry_fill)
                realized += price_pnl
                trades.append(
                    Trade(
                        entry_decision_id=open_trade.entry_decision_id,
                        exit_decision_id=item.change.decision_id,
                        side=open_trade.side,
                        quantity=open_trade.quantity,
                        entry_time_ms=open_trade.entry_time_ms,
                        exit_time_ms=item.fill_time_ms,
                        entry_fill=open_trade.entry_fill,
                        exit_fill=fill_price,
                        price_pnl=price_pnl,
                        fees=open_trade.entry_fee + fee,
                        funding=open_trade.funding,
                    )
                )
                position = ZERO
                if opened_at is not None:
                    held_ms += item.fill_time_ms - opened_at
                    opened_at = None
                open_trade = None
            mark_time, mark_price = mark_at_or_before(marks, time_ms)
            source = "hourly_mark_open" if mark_time == time_ms else "hourly_mark_open_carried_forward"
            observe(time_ms, mark_time, mark_price, source)
            sampled = True
        if time_ms % HOUR_MS == 0 and not sampled:
            if time_ms not in marks:
                missing.append(
                    Discrepancy(
                        area="mark_coverage",
                        timestamp_ms=str(time_ms),
                        field="hourly_mark_open",
                        expected="mark open",
                        recorded="missing",
                        observation="hourly grid sample has no stored mark",
                    )
                )
                return None
            observe(time_ms, time_ms, marks[time_ms], "hourly_mark_open")
    if opened_at is not None:
        held_ms += PERFORMANCE_END_MS - opened_at
    end_mark = marks[PERFORMANCE_END_MS]
    unrealized = position * (end_mark - open_trade.entry_fill) if open_trade else ZERO
    net = realized + unrealized - fees + funding_total
    funding_paid = sum((-flow.cashflow for flow in flows if flow.cashflow < ZERO), ZERO)
    funding_received = sum((flow.cashflow for flow in flows if flow.cashflow > ZERO), ZERO)
    long_net = sum((trade.net_pnl for trade in trades if trade.side == "long"), ZERO)
    short_net = sum((trade.net_pnl for trade in trades if trade.side == "short"), ZERO)
    if open_trade is None:
        open_mtm = ZERO
    else:
        open_mtm = unrealized - open_trade.entry_fee + open_trade.funding
    years = calendar_years(samples, net)
    return Ledger(
        fills=fills,
        pending=pending,
        trades=trades,
        funding=flows,
        samples=samples,
        position_end=position,
        realized=realized,
        unrealized=unrealized,
        fees=fees,
        funding_total=funding_total,
        net=net,
        gross=realized + unrealized,
        funding_paid=funding_paid,
        funding_received=funding_received,
        long_net=long_net,
        short_net=short_net,
        open_mark_to_market=open_mtm,
        max_drawdown=max_drawdown,
        max_drawdown_time_ms=max_drawdown_time,
        max_drawdown_peak_time_ms=max_drawdown_peak_time,
        time_in_market_ms=held_ms,
        max_notional=max_notional,
        years=years,
        estimated_close_cost_on_mark=close_cost_on_mark(position, end_mark, fee_bps, adjustment_bps),
        estimated_close_cost_on_adjusted_fill=close_cost_on_adjusted_fill(
            position, end_mark, fee_bps, adjustment_bps
        ),
    )


def calendar_years(samples: list[Sample], net: Decimal) -> list[dict]:
    by_time = sorted(samples, key=lambda sample: sample.observation_time_ms)

    def cum_exclusive(boundary: int) -> Decimal:
        value = ZERO
        for sample in by_time:
            if sample.observation_time_ms < boundary:
                value = sample.net
            else:
                break
        return value

    def cum_inclusive(boundary: int) -> Decimal:
        value = ZERO
        for sample in by_time:
            if sample.observation_time_ms <= boundary:
                value = sample.net
            else:
                break
        return value

    segments = [
        (2023, True, PERFORMANCE_START_MS, YEAR_2024_MS, False),
        (2024, False, YEAR_2024_MS, YEAR_2025_MS, False),
        (2025, False, YEAR_2025_MS, PERFORMANCE_END_MS, True),
    ]
    years = []
    for year, partial, start, end, final in segments:
        base = cum_exclusive(start)
        current = cum_inclusive(end) if final else cum_exclusive(end)
        years.append(
            {
                "year": year,
                "partial": partial,
                "segment_start_ms": start,
                "segment_end_ms": end,
                "net_pnl": current - base,
            }
        )
    year_sum = sum((row["net_pnl"] for row in years), ZERO)
    if year_sum != net:
        raise SystemExit(f"reconstructed calendar years {year_sum} do not sum to net {net}")
    return years


def optional_ms(text: str) -> int | None:
    text = text.strip()
    if not text:
        return None
    return int(text)


def compare_decisions(expected: list[ExpectedDecision], recorded: list[dict], out: list[Discrepancy]) -> dict:
    economic = 0
    labels = 0
    if len(expected) != len(recorded):
        out.append(
            Discrepancy(
                area="decisions",
                timestamp_ms="",
                field="decision_count",
                expected=str(len(expected)),
                recorded=str(len(recorded)),
                observation="counts are derived from the candle list, not from the recorded entry count",
            )
        )
    by_start = {row["start_ms"]: row for row in (decision_to_public(item) for item in expected)}
    recorded_starts = [int(row["start_time_ms"]) for row in recorded]
    expected_starts = [item.start_ms for item in expected]
    missing_starts = sorted(set(expected_starts) - set(recorded_starts))
    extra_starts = sorted(set(recorded_starts) - set(expected_starts))
    for start in missing_starts:
        out.append(
            Discrepancy(
                area="decisions",
                timestamp_ms=str(start),
                field="presence",
                expected="decision row",
                recorded="absent",
                observation=f"candle {iso(start)} produced an independent decision",
            )
        )
        economic += 1
    for start in extra_starts:
        out.append(
            Discrepancy(
                area="decisions",
                timestamp_ms=str(start),
                field="presence",
                expected="absent",
                recorded="decision row",
                observation="recorded decision has no candle in the warm-up plus evaluation series",
            )
        )
        economic += 1
    comparable = []
    for row in recorded:
        start = int(row["start_time_ms"])
        if start not in by_start:
            continue
        expected_row = by_start[start]
        comparable.append((expected_row, row))
        checks = [
            ("end_time_exclusive_ms", expected_row["end_ms"], row["end_time_exclusive_ms"]),
            ("close", expected_row["close"], row["close"]),
            ("previous_direction", expected_row["previous_direction"], row["previous_direction"]),
            ("direction", expected_row["direction"], row["direction"]),
            ("readiness", expected_row["readiness"], row["readiness"]),
            ("target_kind", expected_row["target_kind"], row["target"]["kind"]),
        ]
        for name, exp, rec in checks:
            match = values_match(exp, rec)
            if not match:
                out.append(
                    Discrepancy(
                        area="decisions",
                        timestamp_ms=str(start),
                        field=name,
                        expected=str(exp),
                        recorded=str(rec),
                        observation=channel_observation(expected_row),
                    )
                )
                economic += 1
        for name in ("entry_high", "entry_low", "exit_high", "exit_low"):
            exp = expected_row[name]
            rec = row[name]
            if exp is None and rec is None:
                continue
            if exp is None or rec is None or not same(exp, rec):
                out.append(
                    Discrepancy(
                        area="decisions",
                        timestamp_ms=str(start),
                        field=name,
                        expected="" if exp is None else dec_text(exp),
                        recorded="" if rec is None else rec,
                        observation=channel_observation(expected_row),
                    )
                )
                economic += 1
        exp_qty = expected_row["target_qty"]
        rec_qty = row["target"].get("quantity")
        if expected_row["target_kind"] == "quantity":
            if rec_qty is None or not same(exp_qty, rec_qty):
                out.append(
                    Discrepancy(
                        area="decisions",
                        timestamp_ms=str(start),
                        field="target_quantity",
                        expected=dec_text(exp_qty),
                        recorded="" if rec_qty is None else rec_qty,
                        observation=channel_observation(expected_row),
                    )
                )
                economic += 1
        if expected_row["reason"] != row["reason"]:
            out.append(
                Discrepancy(
                    area="decision_label",
                    timestamp_ms=str(start),
                    field="reason",
                    expected=expected_row["reason"],
                    recorded=row["reason"],
                    observation="label only; direction and target are compared separately",
                )
            )
            labels += 1
    return {"economic": economic, "labels": labels, "compared": len(comparable)}


def channel_observation(row: dict) -> str:
    return (
        f"close {dec_text(row['close'])}; "
        f"entry window {row['entry_window_start_ms']}..{row['entry_window_end_ms']} "
        f"high {'' if row['entry_high'] is None else dec_text(row['entry_high'])} "
        f"at {row['entry_high_at_ms']} "
        f"low {'' if row['entry_low'] is None else dec_text(row['entry_low'])} "
        f"at {row['entry_low_at_ms']}; "
        f"exit window {row['exit_window_start_ms']}..{row['exit_window_end_ms']} "
        f"high {'' if row['exit_high'] is None else dec_text(row['exit_high'])} "
        f"at {row['exit_high_at_ms']} "
        f"low {'' if row['exit_low'] is None else dec_text(row['exit_low'])} "
        f"at {row['exit_low_at_ms']}"
    )


def decision_to_public(decision: ExpectedDecision) -> dict:
    return {
        "start_ms": decision.start_ms,
        "end_ms": decision.end_ms,
        "close": decision.close,
        "entry_high": decision.entry_high,
        "entry_low": decision.entry_low,
        "exit_high": decision.exit_high,
        "exit_low": decision.exit_low,
        "entry_window_start_ms": decision.entry_window_start_ms,
        "entry_window_end_ms": decision.entry_window_end_ms,
        "exit_window_start_ms": decision.exit_window_start_ms,
        "exit_window_end_ms": decision.exit_window_end_ms,
        "entry_high_at_ms": decision.entry_high_at_ms,
        "entry_low_at_ms": decision.entry_low_at_ms,
        "exit_high_at_ms": decision.exit_high_at_ms,
        "exit_low_at_ms": decision.exit_low_at_ms,
        "previous_direction": decision.previous_direction,
        "direction": decision.direction,
        "readiness": decision.readiness,
        "reason": decision.reason,
        "target_kind": decision.target_kind,
        "target_qty": decision.target_qty,
    }


def compare_field(out: list[Discrepancy], area: str, timestamp: str, field_name: str, expected, recorded, observation: str) -> None:
    if isinstance(expected, Decimal) or isinstance(recorded, Decimal) or _numeric(expected, recorded):
        if same(expected, recorded):
            return
        expected_text = dec_text(D(expected))
        recorded_text = dec_text(D(recorded))
    else:
        if str(expected) == str(recorded):
            return
        expected_text = str(expected)
        recorded_text = str(recorded)
    out.append(
        Discrepancy(
            area=area,
            timestamp_ms=timestamp,
            field=field_name,
            expected=expected_text,
            recorded=recorded_text,
            observation=observation,
        )
    )


def _numeric(expected, recorded) -> bool:
    try:
        D(expected)
        D(recorded)
    except Exception:
        return False
    return True


def compare_fills(ledger: Ledger, recorded_rows: list[dict], out: list[Discrepancy]) -> None:
    if len(ledger.fills) != len(recorded_rows):
        out.append(
            Discrepancy(
                area="fills",
                timestamp_ms="",
                field="fill_count",
                expected=str(len(ledger.fills)),
                recorded=str(len(recorded_rows)),
                observation="fill count comes from reconstructed target changes",
            )
        )
    recorded_by_id = {row["decision_id"]: row for row in recorded_rows}
    expected_ids = [fill.decision_id for fill in ledger.fills]
    for fill in ledger.fills:
        row = recorded_by_id.get(fill.decision_id)
        stamp = str(fill.fill_time_ms)
        if row is None:
            out.append(
                Discrepancy(
                    area="fills",
                    timestamp_ms=stamp,
                    field="presence",
                    expected=fill.decision_id,
                    recorded="absent",
                    observation=f"reference open {dec_text(fill.reference_open)} at {iso(fill.fill_time_ms)}",
                )
            )
            continue
        observation = (
            f"stored trade-1m open at {fill.fill_time_ms} is {dec_text(fill.reference_open)}; "
            f"available_at {fill.available_at_ms}"
        )
        pairs = [
            ("decision_time_ms", fill.decision_time_ms, row["decision_time_ms"]),
            ("available_at_ms", fill.available_at_ms, row["available_at_ms"]),
            ("fill_time_ms", fill.fill_time_ms, row["fill_time_ms"]),
            ("side", fill.side, row["side"]),
            ("quantity", fill.quantity, row["quantity"]),
            ("reference_open", fill.reference_open, row["reference_open"]),
            ("fill_price", fill.fill_price, row["fill_price"]),
            ("fee", fill.fee, row["fee"]),
            ("position_before", fill.position_before, row["position_before"]),
            ("position_after", fill.position_after, row["position_after"]),
        ]
        for name, exp, rec in pairs:
            compare_field(out, "fills", stamp, name, exp, rec, observation)
        compare_field(
            out,
            "fills",
            stamp,
            "source_start_ms",
            "" if fill.source_start_ms is None else fill.source_start_ms,
            row["source_start_ms"],
            observation,
        )
        compare_field(
            out,
            "fills",
            stamp,
            "source_end_ms",
            "" if fill.source_end_ms is None else fill.source_end_ms,
            row["source_end_ms"],
            observation,
        )
    for row in recorded_rows:
        if row["decision_id"] not in expected_ids:
            out.append(
                Discrepancy(
                    area="fills",
                    timestamp_ms=row["fill_time_ms"],
                    field="presence",
                    expected="absent",
                    recorded=row["decision_id"],
                    observation="recorded fill has no reconstructed target change",
                )
            )


def compare_trades(ledger: Ledger, recorded_rows: list[dict], out: list[Discrepancy]) -> None:
    if len(ledger.trades) != len(recorded_rows):
        out.append(
            Discrepancy(
                area="trades",
                timestamp_ms="",
                field="trade_count",
                expected=str(len(ledger.trades)),
                recorded=str(len(recorded_rows)),
                observation="completed round trips only; an open position is not a trade",
            )
        )
    recorded_by_key = {(row["entry_decision_id"], row["exit_decision_id"]): row for row in recorded_rows}
    for trade in ledger.trades:
        key = (trade.entry_decision_id, trade.exit_decision_id)
        row = recorded_by_key.get(key)
        stamp = str(trade.exit_time_ms)
        observation = (
            f"q {dec_text(trade.quantity)} entry {dec_text(trade.entry_fill)} "
            f"exit {dec_text(trade.exit_fill)} price_pnl {dec_text(trade.price_pnl)}"
        )
        if row is None:
            out.append(
                Discrepancy(
                    area="trades",
                    timestamp_ms=stamp,
                    field="presence",
                    expected=f"{key[0]} -> {key[1]}",
                    recorded="absent",
                    observation=observation,
                )
            )
            continue
        pairs = [
            ("side", trade.side, row["side"]),
            ("quantity", trade.quantity, row["quantity"]),
            ("entry_time_ms", trade.entry_time_ms, row["entry_time_ms"]),
            ("exit_time_ms", trade.exit_time_ms, row["exit_time_ms"]),
            ("holding_period_ms", trade.holding_ms, row["holding_period_ms"]),
            ("entry_fill", trade.entry_fill, row["entry_fill"]),
            ("exit_fill", trade.exit_fill, row["exit_fill"]),
            ("price_pnl", trade.price_pnl, row["price_pnl"]),
            ("fees", trade.fees, row["fees"]),
            ("funding", trade.funding, row["funding"]),
            ("net_pnl", trade.net_pnl, row["net_pnl"]),
        ]
        for name, exp, rec in pairs:
            compare_field(out, "trades", stamp, name, exp, rec, observation)


def compare_funding(ledger: Ledger, recorded_rows: list[dict], out: list[Discrepancy]) -> None:
    if len(ledger.funding) != len(recorded_rows):
        out.append(
            Discrepancy(
                area="funding",
                timestamp_ms="",
                field="settlement_count",
                expected=str(len(ledger.funding)),
                recorded=str(len(recorded_rows)),
                observation="in-window settlements only; position is the pre-fill position",
            )
        )
    recorded_by_time = {int(row["settlement_time_ms"]): row for row in recorded_rows}
    for flow in ledger.funding:
        row = recorded_by_time.get(flow.settlement_time_ms)
        stamp = str(flow.settlement_time_ms)
        observation = (
            f"rate {dec_text(flow.funding_rate)} mark {dec_text(flow.mark_price)} "
            f"at {flow.mark_time_ms} method {flow.mark_method}"
        )
        if row is None:
            out.append(
                Discrepancy(
                    area="funding",
                    timestamp_ms=stamp,
                    field="presence",
                    expected="settlement",
                    recorded="absent",
                    observation=observation,
                )
            )
            continue
        pairs = [
            ("signed_position", flow.signed_position, row["signed_position"]),
            ("funding_rate", flow.funding_rate, row["funding_rate"]),
            ("mark_price", flow.mark_price, row["mark_price"]),
            ("mark_time_ms", flow.mark_time_ms, row["mark_time_ms"]),
            ("mark_method", flow.mark_method, row["mark_method"]),
            ("cashflow", flow.cashflow, row["cashflow"]),
        ]
        for name, exp, rec in pairs:
            compare_field(out, "funding", stamp, name, exp, rec, observation)


def compare_pnl(ledger: Ledger, recorded_rows: list[dict], out: list[Discrepancy], book: str) -> None:
    if len(ledger.samples) != len(recorded_rows):
        out.append(
            Discrepancy(
                area=f"pnl:{book}",
                timestamp_ms="",
                field="sample_count",
                expected=str(len(ledger.samples)),
                recorded=str(len(recorded_rows)),
                observation="hourly samples plus a sample after funding and after each fill",
            )
        )
    mismatches = 0
    for sample, row in zip(ledger.samples, recorded_rows):
        checks = [
            ("observation_time_ms", sample.observation_time_ms, row["observation_time_ms"]),
            ("position", sample.position, row["position"]),
            ("cumulative_net_pnl", sample.net, row["cumulative_net_pnl"]),
            ("drawdown_usdt", sample.drawdown, row["drawdown_usdt"]),
            ("mark_price", sample.mark_price, row["mark_price"]),
        ]
        row_bad = False
        for name, exp, rec in checks:
            if name == "observation_time_ms":
                match = int(exp) == int(rec)
            else:
                match = same(exp, rec)
            if not match:
                row_bad = True
                if mismatches < 5:
                    out.append(
                        Discrepancy(
                            area=f"pnl:{book}",
                            timestamp_ms=str(sample.observation_time_ms),
                            field=name,
                            expected=dec_text(D(exp)) if name != "observation_time_ms" else str(exp),
                            recorded=str(rec),
                            observation=f"mark source {sample.mark_source}",
                        )
                    )
        if row_bad:
            mismatches += 1
    if mismatches > 5:
        out.append(
            Discrepancy(
                area=f"pnl:{book}",
                timestamp_ms="",
                field="additional_sample_mismatches",
                expected="0",
                recorded=str(mismatches - 5),
                observation="only the first five sample mismatches are listed",
            )
        )


def compare_summary(ledger: Ledger, summary: dict, out: list[Discrepancy], book: str) -> None:
    pairs = [
        ("net_pnl", ledger.net, summary["net_pnl"]),
        ("gross_price_pnl", ledger.gross, summary["gross_price_pnl"]),
        ("realized_price_pnl", ledger.realized, summary["realized_price_pnl"]),
        ("unrealized_price_pnl", ledger.unrealized, summary["unrealized_price_pnl"]),
        ("fees", ledger.fees, summary["fees"]),
        ("funding", ledger.funding_total, summary["funding"]),
        ("funding_paid", ledger.funding_paid, summary["funding_paid"]),
        ("funding_received", ledger.funding_received, summary["funding_received"]),
        ("long_net_pnl", ledger.long_net, summary["long_net_pnl"]),
        ("short_net_pnl", ledger.short_net, summary["short_net_pnl"]),
        ("open_mark_to_market", ledger.open_mark_to_market, summary["open_mark_to_market"]),
        ("max_drawdown_usdt", ledger.max_drawdown, summary["max_drawdown_usdt"]),
        ("max_drawdown_time_ms", ledger.max_drawdown_time_ms, summary["max_drawdown_time_ms"]),
        ("max_drawdown_peak_time_ms", ledger.max_drawdown_peak_time_ms, summary["max_drawdown_peak_time_ms"]),
        ("time_in_market_ms", ledger.time_in_market_ms, summary["time_in_market_ms"]),
        ("max_notional_usdt", ledger.max_notional, summary["max_notional_usdt"]),
        ("simulated_position_at_end", ledger.position_end, summary["simulated_position_at_end"]),
        ("fills", len(ledger.fills), summary["fills"]),
        ("completed_trades", len(ledger.trades), summary["completed_trades"]),
    ]
    stored_close = D(summary["estimated_close_cost_usdt"])
    on_mark = ledger.estimated_close_cost_on_mark
    on_fill = ledger.estimated_close_cost_on_adjusted_fill
    if stored_close != on_mark or stored_close != on_fill:
        matched = "unadjusted mark" if stored_close == on_mark else "adjusted close fill" if stored_close == on_fill else "neither formula"
        out.append(
            Discrepancy(
                area="close_cost_definition",
                timestamp_ms=str(PERFORMANCE_END_MS),
                field="estimated_close_cost_usdt",
                expected=f"on unadjusted mark {dec_text(on_mark)}; on adjusted close fill {dec_text(on_fill)}",
                recorded=dec_text(stored_close),
                observation=(
                    f"stored figure matches {matched}. "
                    f"Difference between the formulas is {dec_text(on_fill - on_mark)}. "
                    "The figure is disclosed only and is not part of net PnL. "
                    f"Terminal mark and position are in the {book} ledger."
                ),
            )
        )
    for name, exp, rec in pairs:
        compare_field(out, f"summary:{book}", str(PERFORMANCE_END_MS), name, exp, rec, "aggregate reconstructed from candles and auxiliary rows")
    recorded_years = {int(row["year"]): row for row in summary["calendar_years"]}
    for year in ledger.years:
        recorded = recorded_years.get(year["year"])
        if recorded is None:
            out.append(
                Discrepancy(
                    area=f"summary:{book}",
                    timestamp_ms=str(year["segment_start_ms"]),
                    field=f"year_{year['year']}",
                    expected=dec_text(year["net_pnl"]),
                    recorded="absent",
                    observation="year boundary uses exclusive cumulative PnL; the final year includes the terminal sample",
                )
            )
            continue
        compare_field(
            out,
            f"summary:{book}",
            str(year["segment_end_ms"]),
            f"year_{year['year']}_net_pnl",
            year["net_pnl"],
            recorded["net_pnl"],
            "change in cumulative marked net PnL across the year boundary, not exit-year assignment",
        )
    if ledger.trades:
        winners = sorted((trade.net_pnl for trade in ledger.trades if trade.net_pnl > ZERO), reverse=True)
        losers = [trade.net_pnl for trade in ledger.trades if trade.net_pnl < ZERO]
        largest_loss = min(losers) if losers else None
        largest_win = winners[0] if winners else None
        if largest_loss is not None:
            compare_field(
                out,
                f"summary:{book}",
                "",
                "largest_losing_trade",
                largest_loss,
                summary["largest_losing_trade"],
                "minimum completed-trade net PnL in this scenario",
            )
        if largest_win is not None:
            compare_field(
                out,
                f"summary:{book}",
                "",
                "largest_winner_net",
                largest_win,
                summary["largest_winner_net"],
                "maximum completed-trade net PnL in this scenario",
            )
        compare_field(out, f"summary:{book}", "", "winning_trades", len(winners), summary["winning_trades"], "count")
        compare_field(out, f"summary:{book}", "", "losing_trades", len(losers), summary["losing_trades"], "count")


def validate_daily(candles: list[Candle], metadata: dict, out: list[Discrepancy]) -> None:
    if len(candles) != metadata["candle_count"]:
        out.append(
            Discrepancy("dataset", "", "candle_count", str(len(candles)), str(metadata["candle_count"]), "row count")
        )
    previous_end = None
    for candle in candles:
        if candle.end_ms != candle.start_ms + DAY_MS:
            out.append(
                Discrepancy(
                    "dataset",
                    str(candle.start_ms),
                    "duration",
                    str(candle.start_ms + DAY_MS),
                    str(candle.end_ms),
                    "daily candle must span exactly one UTC day",
                )
            )
        if candle.start_ms % DAY_MS != 0:
            out.append(
                Discrepancy(
                    "dataset",
                    str(candle.start_ms),
                    "alignment",
                    "multiple of 86400000",
                    str(candle.start_ms),
                    "start is not UTC midnight",
                )
            )
        if previous_end is not None and candle.start_ms != previous_end:
            out.append(
                Discrepancy(
                    "dataset",
                    str(candle.start_ms),
                    "chain",
                    str(previous_end),
                    str(candle.start_ms),
                    "exclusive end of the previous candle is not this start",
                )
            )
        if candle.high < candle.low or candle.high < candle.close or candle.low > candle.close:
            out.append(
                Discrepancy(
                    "dataset",
                    str(candle.start_ms),
                    "ohlc",
                    "low <= close <= high and low <= high",
                    f"o {dec_text(candle.open)} h {dec_text(candle.high)} l {dec_text(candle.low)} c {dec_text(candle.close)}",
                    "stored OHLC is internally inconsistent",
                )
            )
        previous_end = candle.end_ms


def validate_hours(points: list[dict], out: list[Discrepancy]) -> dict[int, Decimal]:
    marks: dict[int, Decimal] = {}
    for row in points:
        timestamp = int(row["time_ms"])
        if timestamp in marks:
            out.append(Discrepancy("marks", str(timestamp), "duplicate", "one", "two", "duplicate hourly mark"))
        marks[timestamp] = D(row["open"])
    expected = list(range(PERFORMANCE_START_MS, PERFORMANCE_END_MS + HOUR_MS, HOUR_MS))
    for timestamp in expected:
        if timestamp not in marks:
            out.append(
                Discrepancy(
                    "marks",
                    str(timestamp),
                    "coverage",
                    "hourly open",
                    "missing",
                    "performance grid requires this hour",
                )
            )
    extra = sorted(set(marks) - set(expected))
    for timestamp in extra:
        out.append(
            Discrepancy(
                "marks",
                str(timestamp),
                "coverage",
                "absent",
                "present",
                "mark is outside the inclusive performance hour grid",
            )
        )
    return marks


def validate_funding(records: list[dict], out: list[Discrepancy]) -> list[tuple[int, Decimal]]:
    rows = [(int(row["settlement_time_ms"]), D(row["funding_rate"])) for row in records]
    seen = set()
    for index, (timestamp, _rate) in enumerate(rows):
        if timestamp in seen:
            out.append(Discrepancy("funding_file", str(timestamp), "duplicate", "one", "two", "duplicate settlement"))
        seen.add(timestamp)
        if index and timestamp - rows[index - 1][0] != 28_800_000:
            out.append(
                Discrepancy(
                    "funding_file",
                    str(timestamp),
                    "interval",
                    "28800000 after previous stored settlement",
                    str(timestamp - rows[index - 1][0]),
                    "observed gap in the stored file; no settlement was invented",
                )
            )
        if timestamp % HOUR_MS != 0:
            out.append(
                Discrepancy(
                    "funding_file",
                    str(timestamp),
                    "alignment",
                    "hour boundary",
                    str(timestamp),
                    "settlement is not on an hourly mark",
                )
            )
    if rows[0][0] > PERFORMANCE_START_MS or rows[-1][0] < PERFORMANCE_END_MS:
        out.append(
            Discrepancy(
                "funding_file",
                "",
                "span",
                f"cover {PERFORMANCE_START_MS} through the last in-window settlement",
                f"{rows[0][0]}..{rows[-1][0]}",
                "stored funding does not surround the performance window",
            )
        )
    return rows


def validate_minutes(points: list[dict]) -> dict[int, Decimal]:
    minutes: dict[int, Decimal] = {}
    for row in points:
        timestamp = int(row["time_ms"])
        if timestamp in minutes and minutes[timestamp] != D(row["open"]):
            raise SystemExit(f"conflicting trade opens at {timestamp}")
        minutes[timestamp] = D(row["open"])
    return minutes


def fill_public(fill: FillRow) -> dict:
    return {
        "decision_id": fill.decision_id,
        "source_start_ms": "" if fill.source_start_ms is None else fill.source_start_ms,
        "source_end_ms": "" if fill.source_end_ms is None else fill.source_end_ms,
        "decision_time_ms": fill.decision_time_ms,
        "available_at_ms": fill.available_at_ms,
        "fill_time_ms": fill.fill_time_ms,
        "side": fill.side,
        "quantity": dec_text(fill.quantity),
        "reference_open": dec_text(fill.reference_open),
        "fill_price": dec_text(fill.fill_price),
        "fee": dec_text(fill.fee),
        "position_before": dec_text(fill.position_before),
        "position_after": dec_text(fill.position_after),
    }


def trade_public(trade: Trade) -> dict:
    return {
        "entry_decision_id": trade.entry_decision_id,
        "exit_decision_id": trade.exit_decision_id,
        "side": trade.side,
        "quantity": dec_text(trade.quantity),
        "entry_time_ms": trade.entry_time_ms,
        "exit_time_ms": trade.exit_time_ms,
        "holding_period_ms": trade.holding_ms,
        "entry_fill": dec_text(trade.entry_fill),
        "exit_fill": dec_text(trade.exit_fill),
        "price_pnl": dec_text(trade.price_pnl),
        "fees": dec_text(trade.fees),
        "funding": dec_text(trade.funding),
        "net_pnl": dec_text(trade.net_pnl),
    }


def funding_public(flow: FundingFlow) -> dict:
    return {
        "settlement_time_ms": flow.settlement_time_ms,
        "signed_position": dec_text(flow.signed_position),
        "funding_rate": dec_text(flow.funding_rate),
        "mark_price": dec_text(flow.mark_price),
        "mark_time_ms": flow.mark_time_ms,
        "mark_method": flow.mark_method,
        "cashflow": dec_text(flow.cashflow),
    }


def sample_public(sample: Sample) -> dict:
    return {
        "observation_time_ms": sample.observation_time_ms,
        "mark_time_ms": sample.mark_time_ms,
        "mark_price": dec_text(sample.mark_price),
        "mark_source": sample.mark_source,
        "position": dec_text(sample.position),
        "realized_price_pnl": dec_text(sample.realized),
        "unrealized_price_pnl": dec_text(sample.unrealized),
        "cumulative_fees": dec_text(sample.fees),
        "cumulative_funding": dec_text(sample.funding),
        "cumulative_net_pnl": dec_text(sample.net),
        "running_peak": dec_text(sample.running_peak),
        "drawdown_usdt": dec_text(sample.drawdown),
    }


def decision_csv_row(decision: ExpectedDecision, recorded: dict | None, status: str) -> dict:
    return {
        "start_time_ms": decision.start_ms,
        "end_time_exclusive_ms": decision.end_ms,
        "start_iso": iso(decision.start_ms),
        "close": dec_text(decision.close),
        "entry_window_start_ms": "" if decision.entry_window_start_ms is None else decision.entry_window_start_ms,
        "entry_window_end_ms": "" if decision.entry_window_end_ms is None else decision.entry_window_end_ms,
        "entry_high": "" if decision.entry_high is None else dec_text(decision.entry_high),
        "entry_high_at_ms": decision.entry_high_at_ms,
        "entry_low": "" if decision.entry_low is None else dec_text(decision.entry_low),
        "entry_low_at_ms": decision.entry_low_at_ms,
        "exit_window_start_ms": "" if decision.exit_window_start_ms is None else decision.exit_window_start_ms,
        "exit_window_end_ms": "" if decision.exit_window_end_ms is None else decision.exit_window_end_ms,
        "exit_high": "" if decision.exit_high is None else dec_text(decision.exit_high),
        "exit_high_at_ms": decision.exit_high_at_ms,
        "exit_low": "" if decision.exit_low is None else dec_text(decision.exit_low),
        "exit_low_at_ms": decision.exit_low_at_ms,
        "previous_direction": decision.previous_direction,
        "direction": decision.direction,
        "readiness": decision.readiness,
        "reason": decision.reason,
        "target_kind": decision.target_kind,
        "target_qty": "" if decision.target_qty is None else dec_text(decision.target_qty),
        "recorded_direction": "" if recorded is None else recorded["direction"],
        "recorded_target_kind": "" if recorded is None else recorded["target"]["kind"],
        "recorded_target_qty": "" if recorded is None else recorded["target"].get("quantity", ""),
        "comparison": status,
    }


def trade_section(trade: Trade, decisions: dict[str, ExpectedDecision], flows: list[FundingFlow], rank_note: str) -> list[str]:
    entry = decisions[trade.entry_decision_id]
    exit_decision = decisions[trade.exit_decision_id]
    held = [
        flow
        for flow in flows
        if flow.signed_position == trade.quantity and trade.entry_time_ms < flow.settlement_time_ms <= trade.exit_time_ms
    ]
    funding_sum = sum((flow.cashflow for flow in held), ZERO)
    lines = [
        f"### {trade.side} {dec_text(trade.quantity)} BTC, net {dec_text(trade.net_pnl)} USDT",
        "",
        rank_note,
        "",
        f"- entry signal candle {iso(entry.start_ms)} to {iso(entry.end_ms)}, close {dec_text(entry.close)}",
        f"- previous direction {entry.previous_direction}; decision {entry.reason}; new direction {entry.direction}",
        f"- entry channel window {iso(entry.entry_window_start_ms or 0)} inclusive to {iso(entry.entry_window_end_ms or 0)} exclusive",
        f"- entry high {dec_text(entry.entry_high or ZERO)} from daily starts {entry.entry_high_at_ms}",
        f"- entry low {dec_text(entry.entry_low or ZERO)} from daily starts {entry.entry_low_at_ms}",
        f"- exit channel at entry: high {dec_text(entry.exit_high or ZERO)}, low {dec_text(entry.exit_low or ZERO)}",
        f"- exit signal candle {iso(exit_decision.start_ms)} to {iso(exit_decision.end_ms)}, close {dec_text(exit_decision.close)}",
        f"- exit reason {exit_decision.reason}",
        f"- exit channel window {iso(exit_decision.exit_window_start_ms or 0)} inclusive to {iso(exit_decision.exit_window_end_ms or 0)} exclusive",
        f"- exit high {dec_text(exit_decision.exit_high or ZERO)} from daily starts {exit_decision.exit_high_at_ms}",
        f"- exit low {dec_text(exit_decision.exit_low or ZERO)} from daily starts {exit_decision.exit_low_at_ms}",
        f"- entry fill {iso(trade.entry_time_ms)} price {dec_text(trade.entry_fill)}",
        f"- exit fill {iso(trade.exit_time_ms)} price {dec_text(trade.exit_fill)}",
        f"- price PnL = {dec_text(trade.quantity)} * ({dec_text(trade.exit_fill)} - {dec_text(trade.entry_fill)}) = {dec_text(trade.price_pnl)}",
        f"- fees {dec_text(trade.fees)}; funding on {len(held)} settlements summing to {dec_text(funding_sum)}",
        f"- trade funding field {dec_text(trade.funding)}; net = price - fees + funding = {dec_text(trade.net_pnl)}",
        "",
    ]
    if funding_sum != trade.funding:
        lines.append(
            f"Funding attribution mismatch inside this trade: settlements sum to {dec_text(funding_sum)}, trade field is {dec_text(trade.funding)}."
        )
        lines.append("")
    return lines


def write_report(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def main() -> None:
    discrepancies: list[Discrepancy] = []
    dataset = read_json(DATA / "bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01.json")
    decisions_file = read_json(DATA / "bybit-linear-BTCUSDT-D-2023-01-01_2026-01-01-decisions.json")
    trade_file = read_json(DATA / "aux" / "trade-1m.json")
    funding_file = read_json(DATA / "aux" / "funding.json")
    mark_file = read_json(DATA / "aux" / "mark-60.json")

    dataset_hash = sha256_payload(
        {
            "format_version": dataset["metadata"]["format_version"],
            "venue": dataset["metadata"]["venue"],
            "environment": dataset["metadata"]["environment"],
            "product": dataset["metadata"]["product"],
            "symbol": dataset["metadata"]["symbol"],
            "interval": dataset["metadata"]["interval"],
            "candles": dataset["candles"],
        }
    )
    config_hash = sha256_payload(
        {
            "strategy_id": "DonchianDailyV1",
            "strategy_version": "1",
            "venue": "bybit",
            "product": "linear",
            "instrument": "BTCUSDT",
            "interval": "D",
            "entry_lookback": ENTRY_LOOKBACK,
            "exit_lookback": EXIT_LOOKBACK,
            "position_qty": "0.001",
            "allow_short": True,
        }
    )
    trade_hash = sha256_payload(
        {
            "format_version": trade_file["metadata"]["format_version"],
            "venue": trade_file["metadata"]["venue"],
            "environment": trade_file["metadata"]["environment"],
            "product": trade_file["metadata"]["product"],
            "symbol": trade_file["metadata"]["symbol"],
            "series": trade_file["metadata"]["series"],
            "points": trade_file["points"],
        }
    )
    mark_hash = sha256_payload(
        {
            "format_version": mark_file["metadata"]["format_version"],
            "venue": mark_file["metadata"]["venue"],
            "environment": mark_file["metadata"]["environment"],
            "product": mark_file["metadata"]["product"],
            "symbol": mark_file["metadata"]["symbol"],
            "series": mark_file["metadata"]["series"],
            "points": mark_file["points"],
        }
    )
    funding_hash = sha256_payload(
        {
            "format_version": funding_file["metadata"]["format_version"],
            "venue": funding_file["metadata"]["venue"],
            "environment": funding_file["metadata"]["environment"],
            "product": funding_file["metadata"]["product"],
            "symbol": funding_file["metadata"]["symbol"],
            "series": funding_file["metadata"]["series"],
            "records": funding_file["records"],
        }
    )
    for label, actual, recorded in [
        ("daily dataset", dataset_hash, dataset["metadata"]["dataset_hash"]),
        ("signal config", config_hash, decisions_file["run"]["config_hash"]),
        ("trade 1m", trade_hash, trade_file["metadata"]["dataset_hash"]),
        ("mark 60", mark_hash, mark_file["metadata"]["dataset_hash"]),
        ("funding", funding_hash, funding_file["metadata"]["dataset_hash"]),
    ]:
        if actual != recorded:
            discrepancies.append(
                Discrepancy("identity", "", f"{label} hash", actual, recorded, "sha256 of the documented compact payload")
            )
    if decisions_file["run"]["dataset_hash"] != dataset["metadata"]["dataset_hash"]:
        discrepancies.append(
            Discrepancy(
                "identity",
                "",
                "decision log dataset hash",
                dataset["metadata"]["dataset_hash"],
                decisions_file["run"]["dataset_hash"],
                "decision log does not name the candle file that was loaded",
            )
        )

    candles = load_candles(dataset)
    validate_daily(candles, dataset["metadata"], discrepancies)
    marks = validate_hours(mark_file["points"], discrepancies)
    funding_rows = validate_funding(funding_file["records"], discrepancies)
    minutes = validate_minutes(trade_file["points"])

    expected = reconstruct_decisions(candles)
    recorded_by_start = {int(row["start_time_ms"]): row for row in decisions_file["decisions"]}
    decision_stats = compare_decisions(expected, decisions_file["decisions"], discrepancies)
    decision_rows = []
    for decision in expected:
        recorded = recorded_by_start.get(decision.start_ms)
        status = "match"
        if recorded is None:
            status = "missing_recorded"
        elif (
            recorded["direction"] != decision.direction
            or recorded["previous_direction"] != decision.previous_direction
            or recorded["target"]["kind"] != decision.target_kind
            or (
                decision.target_qty is not None
                and not same(decision.target_qty, recorded["target"].get("quantity", "nan"))
            )
        ):
            status = "mismatch"
        decision_rows.append(decision_csv_row(decision, recorded, status))

    changes = target_changes(expected)
    entries = sum(1 for change in changes if change.target_qty != ZERO)
    exits = sum(1 for change in changes if change.target_qty == ZERO)
    directions = [item.direction for item in expected if item.readiness == "ready"]
    final_direction = directions[-1] if directions else "flat"

    missing_evidence: list[Discrepancy] = []
    scheduled = schedule_fills(changes, minutes, missing_evidence)
    discrepancies.extend(missing_evidence)

    scenarios = [
        ScenarioSpec("zero_cost_diagnostic", ZERO, ZERO, "donchian_daily_v1"),
        ScenarioSpec("cost_a", Decimal("6"), Decimal("2"), "donchian_daily_v1"),
    ]
    ledgers: dict[str, Ledger] = {}
    for spec in scenarios:
        ledger = simulate(scheduled, funding_rows, marks, spec.fee_bps, spec.adjustment_bps, discrepancies)
        if ledger is None:
            continue
        ledgers[spec.name] = ledger
        recorded_dir = DATA / "backtest" / "donchian_daily_v1" / spec.name
        compare_fills(ledger, read_csv(recorded_dir / "fills.csv"), discrepancies)
        compare_trades(ledger, read_csv(recorded_dir / "trades.csv"), discrepancies)
        compare_funding(ledger, read_csv(recorded_dir / "funding.csv"), discrepancies)
        compare_pnl(ledger, read_csv(recorded_dir / "pnl.csv"), discrepancies, spec.name)
        compare_summary(ledger, read_json(recorded_dir / "summary.json"), discrepancies, spec.name)
        scenario_dir = OUT / "donchian_daily_v1" / spec.name
        write_csv(scenario_dir / "fills.csv", [fill_public(fill) for fill in ledger.fills], list(fill_public(ledger.fills[0]).keys()) if ledger.fills else ["decision_id"])
        write_csv(
            scenario_dir / "trades.csv",
            [trade_public(trade) for trade in ledger.trades],
            list(trade_public(ledger.trades[0]).keys()) if ledger.trades else ["entry_decision_id"],
        )
        write_csv(
            scenario_dir / "funding.csv",
            [funding_public(flow) for flow in ledger.funding],
            list(funding_public(ledger.funding[0]).keys()) if ledger.funding else ["settlement_time_ms"],
        )
        write_csv(
            scenario_dir / "pnl.csv",
            [sample_public(sample) for sample in ledger.samples],
            list(sample_public(ledger.samples[0]).keys()),
        )

    benchmark_change = TargetChange(
        decision_id="benchmark-fixed-long",
        source_start_ms=None,
        source_end_ms=None,
        decision_time_ms=PERFORMANCE_START_MS,
        target_qty=QTY,
    )
    benchmark_missing: list[Discrepancy] = []
    benchmark_scheduled = schedule_fills([benchmark_change], minutes, benchmark_missing)
    discrepancies.extend(benchmark_missing)
    benchmark = simulate(
        benchmark_scheduled,
        funding_rows,
        marks,
        Decimal("6"),
        Decimal("2"),
        discrepancies,
    )
    if benchmark is not None:
        recorded_dir = DATA / "backtest" / "fixed_long_0.001_btc" / "cost_a"
        compare_fills(benchmark, read_csv(recorded_dir / "fills.csv"), discrepancies)
        compare_trades(benchmark, read_csv(recorded_dir / "trades.csv"), discrepancies)
        compare_funding(benchmark, read_csv(recorded_dir / "funding.csv"), discrepancies)
        compare_pnl(benchmark, read_csv(recorded_dir / "pnl.csv"), discrepancies, "fixed_long_cost_a")
        compare_summary(benchmark, read_json(recorded_dir / "summary.json"), discrepancies, "fixed_long_cost_a")
        scenario_dir = OUT / "fixed_long_0.001_btc" / "cost_a"
        write_csv(scenario_dir / "fills.csv", [fill_public(fill) for fill in benchmark.fills], list(fill_public(benchmark.fills[0]).keys()))
        write_csv(scenario_dir / "trades.csv", [trade_public(trade) for trade in benchmark.trades], ["entry_decision_id"])
        write_csv(scenario_dir / "funding.csv", [funding_public(flow) for flow in benchmark.funding], list(funding_public(benchmark.funding[0]).keys()))
        write_csv(scenario_dir / "pnl.csv", [sample_public(sample) for sample in benchmark.samples], list(sample_public(benchmark.samples[0]).keys()))

    decision_fields = list(decision_rows[0].keys())
    write_csv(OUT / "decisions.csv", decision_rows, decision_fields)
    write_csv(
        OUT / "discrepancies.csv",
        [item.__dict__ for item in discrepancies],
        ["area", "timestamp_ms", "field", "expected", "recorded", "observation"],
    )

    zero = ledgers.get("zero_cost_diagnostic")
    cost_a = ledgers.get("cost_a")
    report = build_report(
        expected,
        changes,
        entries,
        exits,
        final_direction,
        decision_stats,
        discrepancies,
        zero,
        cost_a,
        benchmark,
        dataset_hash,
        config_hash,
        len(minutes),
        len(funding_rows),
        len(marks),
    )
    write_report(OUT / "report.md", report)
    economic = [item for item in discrepancies if item.area != "decision_label" and item.area != "close_cost_definition"]
    print(f"decisions={len(expected)} entries={entries} exits={exits} final={final_direction}")
    print(f"discrepancies={len(discrepancies)} economic_or_data={len(economic)}")
    print(f"wrote={OUT}")


def build_report(
    expected: list[ExpectedDecision],
    changes: list[TargetChange],
    entries: int,
    exits: int,
    final_direction: str,
    decision_stats: dict,
    discrepancies: list[Discrepancy],
    zero: Ledger | None,
    cost_a: Ledger | None,
    benchmark: Ledger | None,
    dataset_hash: str,
    config_hash: str,
    minute_count: int,
    funding_count: int,
    mark_count: int,
) -> str:
    by_id = {item.decision_id: item for item in expected}
    lines = [
        "# Independent audit: DonchianDailyV1 frozen backtest",
        "",
        "Checker: `audit/donchian_daily_v1/reference_check.py`.",
        "",
        "Command, from the repository root:",
        "",
        "```text",
        "python3 audit/donchian_daily_v1/reference_check.py",
        "```",
        "",
        "The checker reads stored JSON only. It does not call the strategy model, the replay, or the backtest accounting code. Comparison tolerances are exact `Decimal` equality. No rounding band is applied. Trailing zeros are not a difference.",
        "",
        "Audited: zero-trading-cost diagnostic (fee 0, adverse adjustment 0, delay 10 seconds, historical funding included) and Cost A (fee 6 bps, adverse adjustment 2 bps, delay 10 seconds, same funding). Cost A fixed-long comparator is included.",
        "",
        "Not audited: `higher_cost` and `delayed_execution` for both books. Their files were not used as evidence.",
        "",
        "## Identity",
        "",
        f"- daily candles: {len(expected) + 0} model rows from the stored series; dataset hash `{dataset_hash}`",
        f"- signal config hash `{config_hash}`",
        f"- trade-1m points stored: {minute_count}. This is a sparse tape of required minutes, not a continuous three-year tape.",
        f"- funding records stored: {funding_count}. Observed successive gap is 8 hours on every pair in the file.",
        f"- hourly marks stored: {mark_count}. The inclusive performance grid is checked hour by hour.",
        f"- performance window: {iso(PERFORMANCE_START_MS)} inclusive through {iso(PERFORMANCE_END_MS)} exclusive.",
        "",
        "## Decisions",
        "",
        f"Independent count: {len(expected)} candle decisions, of which {sum(1 for item in expected if item.readiness == 'not_ready')} are warm-up. Target changes: {len(changes)}. Entries (non-zero targets): {entries}. Exits (zero targets): {exits}. Final direction: {final_direction}.",
        "",
        f"Compared with the recorded log: {decision_stats['compared']} aligned candles. Economic decision mismatches: {decision_stats['economic']}. Reason-label mismatches: {decision_stats['labels']}.",
        "",
        "Rules applied: prior 20 daily highs/lows for entry, prior 10 for exit, current candle excluded, strict inequalities, shorting enabled, quantity 0.001 BTC, no same-candle reversal, no resize. The 20 candles immediately before 2023-01-21 are warm-up and produce no target.",
        "",
    ]
    if zero is not None:
        lines.extend(trade_narrative(zero, by_id, "zero-trading-cost diagnostic"))
        lines.extend(
            [
                "## Aggregates",
                "",
                "| Book | Scenario | Net PnL | Gross price | Fees | Funding | Max drawdown | Position at end |",
                "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |",
                f"| DonchianDailyV1 | zero-trading-cost | {dec_text(zero.net)} | {dec_text(zero.gross)} | {dec_text(zero.fees)} | {dec_text(zero.funding_total)} | {dec_text(zero.max_drawdown)} | {dec_text(zero.position_end)} |",
            ]
        )
        if cost_a is not None:
            lines.append(
                f"| DonchianDailyV1 | Cost A | {dec_text(cost_a.net)} | {dec_text(cost_a.gross)} | {dec_text(cost_a.fees)} | {dec_text(cost_a.funding_total)} | {dec_text(cost_a.max_drawdown)} | {dec_text(cost_a.position_end)} |"
            )
        if benchmark is not None:
            lines.append(
                f"| Fixed long 0.001 BTC | Cost A | {dec_text(benchmark.net)} | {dec_text(benchmark.gross)} | {dec_text(benchmark.fees)} | {dec_text(benchmark.funding_total)} | {dec_text(benchmark.max_drawdown)} | {dec_text(benchmark.position_end)} |"
            )
        lines.append("")
        lines.append("Calendar attribution is the change in sampled cumulative net PnL. A sample exactly at 1 January belongs to the year that is starting. 2023 starts on 21 January.")
        lines.append("")
        for year in zero.years:
            lines.append(
                f"- zero-trading-cost {year['year']}: {dec_text(year['net_pnl'])} from {iso(year['segment_start_ms'])} to {iso(year['segment_end_ms'])}"
            )
        if cost_a is not None:
            for year in cost_a.years:
                lines.append(f"- Cost A {year['year']}: {dec_text(year['net_pnl'])}")
        lines.append("")
        lines.append(
            f"Zero-trading-cost long trades {dec_text(zero.long_net)}; short trades {dec_text(zero.short_net)}."
        )
        lines.append("")
    if benchmark is not None and benchmark.fills:
        fill = benchmark.fills[0]
        lines.extend(
            [
                "## Cost A fixed long",
                "",
                f"Entry decision time is the performance start, {iso(PERFORMANCE_START_MS)}. Available at {iso(fill.available_at_ms)}. Fill at the stored one-minute open {iso(fill.fill_time_ms)}, reference {dec_text(fill.reference_open)}, adjusted fill {dec_text(fill.fill_price)}, fee {dec_text(fill.fee)}.",
                "",
                f"The position is still long {dec_text(benchmark.position_end)} at the cutoff. Unrealized price PnL {dec_text(benchmark.unrealized)} is inside net PnL. No exit was invented.",
                "",
                f"Unadjusted-mark close estimate, abs(quantity) * terminal mark * (6+2) / 10000: {dec_text(benchmark.estimated_close_cost_on_mark)}.",
                f"Adjusted-close estimate, gap from the mark to an adverse sell plus a fee on that sell: {dec_text(benchmark.estimated_close_cost_on_adjusted_fill)}.",
                "The stored summary matches the adjusted-close estimate. The two estimates differ by "
                f"{dec_text(benchmark.estimated_close_cost_on_mark - benchmark.estimated_close_cost_on_adjusted_fill)} USDT. Neither is in net PnL.",
                "",
            ]
        )
    economic = [
        item
        for item in discrepancies
        if item.area not in {"decision_label", "close_cost_definition"}
    ]
    lines.extend(["## Discrepancies", ""])
    if not discrepancies:
        lines.append("No differences.")
    else:
        lines.append(f"Total recorded differences: {len(discrepancies)}. Differences other than reason labels and the disclosed close-cost formula: {len(economic)}.")
        lines.append("")
        shown = discrepancies[:40]
        for item in shown:
            lines.append(
                f"- `{item.area}` {item.field} at `{item.timestamp_ms}`: expected `{item.expected}`, recorded `{item.recorded}`. {item.observation}"
            )
        if len(discrepancies) > len(shown):
            lines.append(f"- {len(discrepancies) - len(shown)} further rows are in `discrepancies.csv`.")
        lines.append("")
    lines.extend(
        [
            "## Conclusions",
            "",
            "### 1. Decision correctness",
            "",
        ]
    )
    if decision_stats["economic"] == 0 and not any(item.area == "decisions" for item in discrepancies):
        lines.append(
            "The stored daily candles produce the recorded direction, target, channel bounds, and timestamps on every warm-up and evaluated candle, including no-change rows. The independent entry count is "
            f"{entries} and the independent exit count is {exits}."
        )
    else:
        lines.append("The reconstructed decisions do not match the recorded log. See the discrepancy list. No rule was changed to force a match.")
    lines.extend(["", "### 2. Simulated execution and accounting", ""])
    if not economic:
        lines.append(
            "For the zero-trading-cost diagnostic and Cost A, independently selected one-minute opens, fees, adverse adjustments, funding cash flows, trade nets, calendar splits, and hourly-sampled drawdowns match the stored ledgers. The Cost A fixed-long comparator matches on the same basis, including its open terminal position. Execution-price adjustment is inside the fill price and is not subtracted again. The disclosed estimated closing cost is the one figure with two defensible formulas; it is not part of net PnL, and the difference is quantified above."
        )
    else:
        lines.append("Execution or accounting does not fully match. The differences are listed above and were not patched in the original implementation.")
    lines.extend(
        [
            "",
            "### 3. What remains unverified",
            "",
            "The one-minute tape is sparse. Only the minutes required by these fills were checked, and only because those rows are present. Liquidity, partial fills, and live order behavior are not established. Fee and adverse-adjustment inputs are the prescribed research numbers. The higher-cost and delayed-execution scenarios were not reconstructed. There is no capital, margin, or liquidation model. This audit does not use the 2026 holdout.",
            "",
        ]
    )
    if not economic and decision_stats["economic"] == 0:
        lines.extend(
            [
                "## Screen result",
                "",
                "DonchianDailyV1, daily 20/10, long/short, fixed 0.001 BTC: failed the 2023–2025 economic screen under the tested assumptions. Not approved for live deployment.",
                "",
            ]
        )
    else:
        lines.extend(
            [
                "## Screen result",
                "",
                "The stored screen result is not confirmed, because the reconstruction disagrees. Live deployment remains unjustified. The original implementation was not modified.",
                "",
            ]
        )
    return "\n".join(lines) + "\n"


def trade_narrative(zero: Ledger, by_id: dict[str, ExpectedDecision], scenario_name: str) -> list[str]:
    ranked = sorted(zero.trades, key=lambda trade: trade.net_pnl)
    loser = ranked[0]
    winner = ranked[-1]
    shorts_2025 = [
        trade
        for trade in zero.trades
        if trade.side == "short" and YEAR_2025_MS <= trade.exit_time_ms < PERFORMANCE_END_MS
    ]
    lines = [
        "## Individual trades",
        "",
        f"Ranking scenario: {scenario_name}. Trade net includes historical funding and, in this scenario, zero fees and zero adverse adjustment. Cost A is reconciled in the ledgers but is not the ranking scenario.",
        "",
        "### Largest losing trade",
        "",
    ]
    lines.extend(trade_section(loser, by_id, zero.funding, "Selected as the minimum zero-trading-cost trade net."))
    lines.append("### Largest winning trade")
    lines.append("")
    lines.extend(trade_section(winner, by_id, zero.funding, "Selected as the maximum zero-trading-cost trade net."))
    lines.append("### 2025 short trades")
    lines.append("")
    lines.append(
        f"{len(shorts_2025)} short round trips exit on or after 2025-01-01 and before 2026-01-01. "
        "A short that exits in 2024 is not in this list, even if its funding continued to affect the 2025 calendar split through the mark path of other positions."
    )
    lines.append("")
    shorts_2025_sorted = sorted(shorts_2025, key=lambda trade: trade.net_pnl)
    for trade in shorts_2025_sorted:
        lines.extend(trade_section(trade, by_id, zero.funding, "Exit fill is inside 2025."))
    short_sum = sum((trade.net_pnl for trade in shorts_2025), ZERO)
    lines.append(f"Sum of these 2025-exit short nets: {dec_text(short_sum)}.")
    lines.append("")
    lines.append("### All reconstructed zero-trading-cost trades")
    lines.append("")
    lines.append("| Exit | Side | Price PnL | Funding | Net |")
    lines.append("| --- | --- | ---: | ---: | ---: |")
    for trade in zero.trades:
        lines.append(
            f"| {iso(trade.exit_time_ms)} | {trade.side} | {dec_text(trade.price_pnl)} | {dec_text(trade.funding)} | {dec_text(trade.net_pnl)} |"
        )
    lines.append("")
    return lines


if __name__ == "__main__":
    main()
