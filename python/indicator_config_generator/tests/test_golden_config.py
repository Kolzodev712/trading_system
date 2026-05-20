"""Committed config/indicators.json must match default generator output."""

from __future__ import annotations

import json
from pathlib import Path

from indicator_config_generator.builder import TEMPLATE_DEFAULT
from indicator_config_generator.generator import build_default_file
from indicator_config_generator.models import IndicatorsFile
from indicator_config_generator.validators import validate_indicators_file


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def test_committed_indicators_json_matches_default_generator() -> None:
    path = _repo_root() / "config" / "indicators.json"
    assert path.is_file(), f"missing fixture {path}"

    committed = IndicatorsFile.model_validate(json.loads(path.read_text(encoding="utf-8")))
    assert validate_indicators_file(committed) == []

    pairs = sorted({entry.pair for entry in committed.indicators})
    generated = build_default_file(pairs, templates=[TEMPLATE_DEFAULT])
    assert generated.model_dump(mode="json") == committed.model_dump(mode="json")


def test_composites_template_valid() -> None:
    file = build_default_file(["BTCUSDT"], templates=["default", "composites"])
    assert validate_indicators_file(file) == []
    ids = {e.id for e in file.indicators}
    assert "golden_cross_BTCUSDT" in ids
    assert "death_cross_BTCUSDT" in ids
