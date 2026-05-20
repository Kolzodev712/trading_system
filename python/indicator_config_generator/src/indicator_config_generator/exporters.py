"""Export indicators to JSON."""

from __future__ import annotations

import json
from pathlib import Path

from indicator_config_generator.models import IndicatorsFile


def export_to_json(file: IndicatorsFile, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = file.model_dump(mode="json")
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
