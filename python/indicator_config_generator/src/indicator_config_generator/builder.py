"""Builders for system-wide indicators (no strategy-specific indicators)."""

from __future__ import annotations

from indicator_config_generator.expressions import subtract_refs
from decimal import Decimal

from indicator_config_generator.models import (
    CompositeIndicatorEntry,
    IndicatorEntry,
    LevelPricingRule,
    PriceSource,
    SimpleIndicatorEntry,
)


def simple_id(indicator_type: str, period: int, pair: str) -> str:
    """Canonical ID: `{type}_{period}_{pair}` (e.g. sma_20_BTCUSDT)."""
    return f"{indicator_type.lower()}_{period}_{pair}"


def template_name(indicator_type: str, period: int) -> str:
    """Logical name used by levels resolver: `sma_20` → `sma_20_{pair}`."""
    return f"{indicator_type.lower()}_{period}"


def sma(pair: str, period: int = 20, source: PriceSource = PriceSource.close) -> SimpleIndicatorEntry:
    return SimpleIndicatorEntry(
        id=simple_id("sma", period, pair),
        name="SMA",
        pair=pair,
        period=period,
        source=source,
    )


def ema(pair: str, period: int = 12, source: PriceSource = PriceSource.close) -> SimpleIndicatorEntry:
    return SimpleIndicatorEntry(
        id=simple_id("ema", period, pair),
        name="EMA",
        pair=pair,
        period=period,
        source=source,
    )


def golden_cross(pair: str) -> CompositeIndicatorEntry:
    """EMA50 − EMA200 (depends on simple indicators for the same pair)."""
    ema50 = simple_id("ema", 50, pair)
    ema200 = simple_id("ema", 200, pair)
    return CompositeIndicatorEntry(
        id=f"golden_cross_{pair}",
        name="Golden Cross",
        pair=pair,
        expression=subtract_refs(ema50, ema200),
        dependencies=[ema50, ema200],
    )


def default_level_pricing() -> list[LevelPricingRule]:
    """Default bindings for sample levels (see config/levels.json template ids)."""
    return [
        LevelPricingRule(
            level_id="buy_level_1",
            indicator_ref="sma_20",
            offset_type="PercentBelow",
            offset_value=Decimal("0.5"),
        ),
        LevelPricingRule(
            level_id="sell_level_1",
            indicator_ref="sma_20",
            offset_type="PercentAbove",
            offset_value=Decimal("0.5"),
        ),
    ]


# Template sets (combine with --templates default,composites,macd)
TEMPLATE_DEFAULT = "default"
TEMPLATE_COMPOSITES = "composites"
TEMPLATE_MACD = "macd"
ALL_TEMPLATES = (TEMPLATE_DEFAULT, TEMPLATE_COMPOSITES, TEMPLATE_MACD)


def death_cross(pair: str) -> CompositeIndicatorEntry:
    """EMA200 − EMA50."""
    ema50 = simple_id("ema", 50, pair)
    ema200 = simple_id("ema", 200, pair)
    return CompositeIndicatorEntry(
        id=f"death_cross_{pair}",
        name="Death Cross",
        pair=pair,
        expression=subtract_refs(ema200, ema50),
        dependencies=[ema50, ema200],
    )


def macd_line(pair: str) -> CompositeIndicatorEntry:
    """MACD line: EMA12 − EMA26."""
    ema12 = simple_id("ema", 12, pair)
    ema26 = simple_id("ema", 26, pair)
    return CompositeIndicatorEntry(
        id=f"macd_line_{pair}",
        name="MACD Line",
        pair=pair,
        expression=subtract_refs(ema12, ema26),
        dependencies=[ema12, ema26],
    )


def _add(entries: dict[str, IndicatorEntry], entry: IndicatorEntry) -> None:
    entries[entry.id] = entry


def indicators_for_pairs(
    pairs: list[str],
    templates: list[str] | None = None,
) -> list[IndicatorEntry]:
    """Build indicators for pairs. Default template: SMA20 + EMA12 per pair."""
    names = {t.strip().lower() for t in (templates or [TEMPLATE_DEFAULT])}
    unknown = names - set(ALL_TEMPLATES)
    if unknown:
        raise ValueError(f"unknown templates: {sorted(unknown)}; choose from {ALL_TEMPLATES}")

    by_id: dict[str, IndicatorEntry] = {}
    for pair in pairs:
        if TEMPLATE_DEFAULT in names:
            _add(by_id, sma(pair, period=20))
            _add(by_id, ema(pair, period=12))
        if TEMPLATE_MACD in names:
            _add(by_id, ema(pair, period=12))
            _add(by_id, ema(pair, period=26))
            _add(by_id, macd_line(pair))
        if TEMPLATE_COMPOSITES in names:
            _add(by_id, ema(pair, period=50))
            _add(by_id, ema(pair, period=200))
            _add(by_id, golden_cross(pair))
            _add(by_id, death_cross(pair))

    # Simple indicators before composites (validators + Rust load order).
    simples = [e for e in by_id.values() if isinstance(e, SimpleIndicatorEntry)]
    composites = [e for e in by_id.values() if isinstance(e, CompositeIndicatorEntry)]
    return simples + composites


def default_indicators_for_pairs(pairs: list[str]) -> list[IndicatorEntry]:
    """Standard system indicators referenced by levels (`sma_20`, `ema_12`)."""
    return indicators_for_pairs(pairs, templates=[TEMPLATE_DEFAULT])


