"""Pydantic models aligned with Rust indicator_engine config types."""

from __future__ import annotations

from enum import Enum
from typing import Annotated, Literal, Union

from decimal import Decimal

from pydantic import BaseModel, Field, field_validator


class PriceSource(str, Enum):
    """Matches Rust `PriceSource` (serde lowercase)."""

    close = "close"
    open = "open"
    high = "high"
    low = "low"
    volume = "volume"
    typical_price = "typical_price"
    weighted_price = "weighted_price"


class IndicatorExpression(BaseModel):
    """Expression tree — mirrors Rust `IndicatorExpression` (tag = type)."""

    type: str
    indicator_id: str | None = None
    value: str | None = None
    left: IndicatorExpression | None = None
    right: IndicatorExpression | None = None
    expr: IndicatorExpression | None = None
    condition: IndicatorExpression | None = None
    true_expr: IndicatorExpression | None = None
    false_expr: IndicatorExpression | None = None

    model_config = {"extra": "forbid"}


class SimpleIndicatorEntry(BaseModel):
    kind: Literal["simple"] = "simple"
    id: str
    name: str
    pair: str
    period: int
    source: PriceSource = PriceSource.close

    @field_validator("period")
    @classmethod
    def period_positive(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("period must be > 0")
        return v


class CompositeIndicatorEntry(BaseModel):
    kind: Literal["composite"] = "composite"
    id: str
    name: str
    pair: str
    expression: IndicatorExpression
    dependencies: list[str]

    @field_validator("dependencies")
    @classmethod
    def deps_non_empty(cls, v: list[str]) -> list[str]:
        if not v:
            raise ValueError("composite indicator must have at least one dependency")
        return v


IndicatorEntry = Annotated[
    Union[SimpleIndicatorEntry, CompositeIndicatorEntry],
    Field(discriminator="kind"),
]


class LevelPricingRule(BaseModel):
    """Binds a level template id to an indicator ref + price offset (not in levels.json)."""

    level_id: str
    indicator_ref: str
    offset_type: str
    offset_value: Decimal


class IndicatorsFile(BaseModel):
    """Top-level `config/indicators.json` envelope."""

    version: int = 1
    indicators: list[IndicatorEntry]
    level_pricing: list[LevelPricingRule] = Field(default_factory=list)
