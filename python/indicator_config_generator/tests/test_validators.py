from indicator_config_generator.builder import default_indicators_for_pairs, golden_cross, sma
from indicator_config_generator.models import IndicatorsFile
from indicator_config_generator.validators import validate_indicators_file


def test_default_indicators_valid() -> None:
    file = IndicatorsFile(version=1, indicators=default_indicators_for_pairs(["BTCUSDT"]))
    assert validate_indicators_file(file) == []


def test_golden_cross_requires_simple_deps() -> None:
    pair = "BTCUSDT"
    file = IndicatorsFile(
        version=1,
        indicators=[golden_cross(pair), sma(pair, 50), sma(pair, 200)],
    )
    # ema deps missing — should error
    errors = validate_indicators_file(file)
    assert any("ema_50" in e for e in errors)
