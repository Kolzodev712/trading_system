"""Independent reference for BTCPriceMALongFlatV1 signal math (offline audit).

Does not import the trading engine. Mirrors f64 log-spread semantics used in Rust.
"""

from __future__ import annotations

import math
from decimal import Decimal
from typing import Sequence


def decimal_ln(value: Decimal) -> float:
    if value <= 0:
        raise ValueError(f"log is undefined for non-positive price {value}")
    return math.log(float(value))


def log_spread(closes: Sequence[Decimal]) -> float:
    if not closes:
        raise ValueError("empty close window")
    n = len(closes)
    total = sum(decimal_ln(c) for c in closes)
    current = decimal_ln(closes[-1])
    return current - total / n


def wants_long(closes: Sequence[Decimal]) -> bool:
    return log_spread(closes) > 0.0


def geometric_mean(closes: Sequence[Decimal]) -> Decimal:
    n = len(closes)
    mean_ln = sum(decimal_ln(c) for c in closes) / n
    return Decimal(str(math.exp(mean_ln)))
