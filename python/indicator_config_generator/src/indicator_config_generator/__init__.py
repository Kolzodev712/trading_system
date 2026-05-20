"""Build-time indicator configuration generator."""

from indicator_config_generator.builder import default_indicators_for_pairs
from indicator_config_generator.models import IndicatorsFile

__all__ = ["IndicatorsFile", "default_indicators_for_pairs"]
