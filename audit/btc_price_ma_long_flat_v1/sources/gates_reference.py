"""Reference formulas for the preregistered BTCPriceMALongFlatV1 development screen."""

from __future__ import annotations

from decimal import Decimal
from typing import Iterable, Tuple

BENCHMARK_COST_A_NET = Decimal("48.360542973936")
BENCHMARK_COST_A_DRAWDOWN = Decimal("44.3887151092231")
MIN_PROFIT_FRACTION = Decimal("0.8")
MAX_DRAWDOWN_FRACTION = Decimal("0.7")
MIN_COMPLETED_ROUND_TRIPS = 20


def min_primary_net_for_retention_gate() -> Decimal:
    return BENCHMARK_COST_A_NET * MIN_PROFIT_FRACTION


def max_primary_drawdown_gate() -> Decimal:
    return BENCHMARK_COST_A_DRAWDOWN * MAX_DRAWDOWN_FRACTION


def notional_exposure(samples: Iterable[Tuple[Decimal, Decimal]]) -> Decimal:
    total = Decimal("0")
    for position, mark in samples:
        total += abs(position) * mark
    return total


def scaled_passive_benchmark_net(
    candidate_samples: Iterable[Tuple[Decimal, Decimal]],
    benchmark_samples: Iterable[Tuple[Decimal, Decimal]],
    benchmark_net: Decimal = BENCHMARK_COST_A_NET,
) -> Tuple[Decimal, Decimal]:
    bench_exposure = notional_exposure(benchmark_samples)
    if bench_exposure == 0:
        raise ValueError("benchmark notional exposure is zero")
    f = notional_exposure(candidate_samples) / bench_exposure
    return benchmark_net * f, f
