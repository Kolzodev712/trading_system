"""Default generation entrypoints."""

from __future__ import annotations

from pathlib import Path

from indicator_config_generator.builder import default_level_pricing, indicators_for_pairs
from indicator_config_generator.exporters import export_to_json
from indicator_config_generator.models import IndicatorsFile
from indicator_config_generator.mongo import MongoSettings, MongoSyncCounts, sync_to_mongo
from indicator_config_generator.validators import validate_indicators_file


def build_default_file(
    pairs: list[str],
    *,
    templates: list[str] | None = None,
) -> IndicatorsFile:
    return IndicatorsFile(
        version=1,
        indicators=indicators_for_pairs(pairs, templates=templates),
        level_pricing=default_level_pricing(),
    )


def publish_indicators(
    file: IndicatorsFile,
    output: Path,
    *,
    mongo: MongoSettings | None = None,
) -> tuple[Path, MongoSyncCounts | None]:
    """Write JSON file and optionally upsert the same indicators into MongoDB."""
    errors = validate_indicators_file(file)
    if errors:
        raise ValueError("validation failed:\n" + "\n".join(errors))
    export_to_json(file, output)
    mongo_counts: MongoSyncCounts | None = None
    if mongo is not None:
        mongo_counts = sync_to_mongo(file, mongo)
    return output, mongo_counts


def generate_default(
    pairs: list[str],
    output: Path,
    *,
    templates: list[str] | None = None,
    mongo: MongoSettings | None = None,
) -> tuple[IndicatorsFile, MongoSyncCounts | None]:
    file = build_default_file(pairs, templates=templates)
    _, mongo_counts = publish_indicators(file, output, mongo=mongo)
    return file, mongo_counts
