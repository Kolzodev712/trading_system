"""Expression helpers aligned with Rust `IndicatorExpression` JSON."""

from __future__ import annotations

from indicator_config_generator.models import IndicatorExpression


def ref(indicator_id: str) -> IndicatorExpression:
    return IndicatorExpression(type="simple", indicator_id=indicator_id)


def constant(value: str) -> IndicatorExpression:
    return IndicatorExpression(type="constant", value=value)


def subtract_refs(left_id: str, right_id: str) -> IndicatorExpression:
    return IndicatorExpression(
        type="subtract",
        left=ref(left_id),
        right=ref(right_id),
    )
