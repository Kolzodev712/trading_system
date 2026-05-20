"""CLI: indicator-generate / indicator-validate."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from indicator_config_generator.generator import generate_default
from indicator_config_generator.models import IndicatorsFile
from indicator_config_generator.mongo import MongoSettings
from indicator_config_generator.validators import validate_indicators_file


def _parse_pairs(raw: str) -> list[str]:
    return [p.strip().upper().replace("/", "") for p in raw.split(",") if p.strip()]


def generate(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="Generate config/indicators.json")
    parser.add_argument(
        "--pairs",
        required=True,
        help="Comma-separated pairs (e.g. BTCUSDT,ETHUSDT)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("config/indicators.json"),
        help="Output JSON path",
    )
    parser.add_argument(
        "--bot-config",
        type=Path,
        default=Path("config/bot.json"),
        help="Bot config with mongo.connection_string and mongo.database",
    )
    parser.add_argument(
        "--mongo-uri",
        default=None,
        help="MongoDB URI (overrides bot-config)",
    )
    parser.add_argument(
        "--mongo-database",
        default=None,
        help="MongoDB database name (overrides bot-config)",
    )
    parser.add_argument(
        "--no-mongo",
        action="store_true",
        help="Only write JSON file; do not push to MongoDB",
    )
    parser.add_argument(
        "--templates",
        default="default",
        help="Comma-separated template sets: default, composites, macd (default: default)",
    )
    args = parser.parse_args(argv)
    pairs = _parse_pairs(args.pairs)
    templates = [t.strip().lower() for t in args.templates.split(",") if t.strip()]

    mongo: MongoSettings | None = None
    if not args.no_mongo:
        if args.mongo_uri and args.mongo_database:
            mongo = MongoSettings(
                connection_string=args.mongo_uri,
                database=args.mongo_database,
            )
        elif args.bot_config.is_file():
            mongo = MongoSettings.from_bot_config(args.bot_config)
        else:
            print(
                f"No MongoDB settings: pass --mongo-uri/--mongo-database or provide {args.bot_config}",
                file=sys.stderr,
            )

    file, mongo_counts = generate_default(
        pairs, args.output, templates=templates, mongo=mongo
    )
    from indicator_config_generator.models import SimpleIndicatorEntry

    n_simple = sum(1 for i in file.indicators if isinstance(i, SimpleIndicatorEntry))
    n_comp = len(file.indicators) - n_simple
    print(
        f"Wrote {len(file.indicators)} indicator(s) ({n_simple} simple, {n_comp} composite), "
        f"{len(file.level_pricing)} level_pricing rule(s) for {len(pairs)} pair(s) → {args.output}"
    )
    if mongo is not None and mongo_counts is not None:
        print(
            f"MongoDB {mongo.database}.indicators: "
            f"{mongo_counts.indicators} indicator(s), "
            f"{mongo_counts.level_pricing} level_pricing"
        )


def validate(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="Validate config/indicators.json")
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("config/indicators.json"),
        help="Indicators JSON path",
    )
    args = parser.parse_args(argv)
    data = json.loads(args.config.read_text(encoding="utf-8"))
    file = IndicatorsFile.model_validate(data)
    errors = validate_indicators_file(file)
    if errors:
        for err in errors:
            print(err, file=sys.stderr)
        sys.exit(1)
    print(f"OK: {len(file.indicators)} indicator(s) in {args.config}")
