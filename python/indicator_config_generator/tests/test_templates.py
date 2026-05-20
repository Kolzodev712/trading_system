from indicator_config_generator.builder import indicators_for_pairs, macd_line, sma
from indicator_config_generator.models import CompositeIndicatorEntry, IndicatorsFile
from indicator_config_generator.validators import validate_indicators_file


def test_macd_template() -> None:
    file = IndicatorsFile(
        version=1,
        indicators=indicators_for_pairs(["BTCUSDT"], templates=["macd"]),
    )
    assert validate_indicators_file(file) == []
    assert any(isinstance(e, CompositeIndicatorEntry) and e.id == "macd_line_BTCUSDT" for e in file.indicators)


def test_unknown_template_raises() -> None:
    try:
        indicators_for_pairs(["BTCUSDT"], templates=["nope"])
    except ValueError as e:
        assert "unknown templates" in str(e)
    else:
        raise AssertionError("expected ValueError")
