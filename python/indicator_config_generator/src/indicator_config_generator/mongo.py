"""Push indicator configs to MongoDB (same shape as Rust `IndicatorPersistence`)."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from indicator_config_generator.models import (
    CompositeIndicatorEntry,
    IndicatorEntry,
    IndicatorsFile,
    LevelPricingRule,
    PriceSource,
    SimpleIndicatorEntry,
)

INDICATORS_COLLECTION = "indicators"

# Rust stores `format!("{:?}", PriceSource)` in MongoDB.
_PRICE_SOURCE_MONGO: dict[PriceSource, str] = {
    PriceSource.close: "Close",
    PriceSource.open: "Open",
    PriceSource.high: "High",
    PriceSource.low: "Low",
    PriceSource.volume: "Volume",
    PriceSource.typical_price: "TypicalPrice",
    PriceSource.weighted_price: "WeightedPrice",
}


@dataclass(frozen=True)
class MongoSettings:
    connection_string: str
    database: str

    @classmethod
    def from_bot_config(cls, path: Path) -> MongoSettings:
        data = json.loads(path.read_text(encoding="utf-8"))
        mongo = data.get("mongo") or {}
        if not mongo.get("enabled", True):
            raise ValueError(f"MongoDB disabled in {path}")
        connection_string = mongo.get("connection_string")
        database = mongo.get("database")
        if not connection_string or not database:
            raise ValueError(f"mongo.connection_string and mongo.database required in {path}")
        return cls(connection_string=connection_string, database=database)


def simple_entry_to_mongo_doc(entry: SimpleIndicatorEntry, now: int) -> dict[str, Any]:
    return {
        "_id": entry.id,
        "indicator_type": "simple",
        "name": entry.name,
        "pair": entry.pair,
        "period": entry.period,
        "source": _PRICE_SOURCE_MONGO[entry.source],
        "created_at": now,
        "updated_at": now,
    }


def composite_entry_to_mongo_doc(entry: CompositeIndicatorEntry, now: int) -> dict[str, Any]:
    return {
        "_id": entry.id,
        "indicator_type": "composite",
        "name": entry.name,
        "pair": entry.pair,
        "expression": entry.expression.model_dump(mode="json", exclude_none=True),
        "dependencies": entry.dependencies,
        "created_at": now,
        "updated_at": now,
    }


def entry_to_mongo_doc(entry: IndicatorEntry, now: int) -> dict[str, Any]:
    if isinstance(entry, SimpleIndicatorEntry):
        return simple_entry_to_mongo_doc(entry, now)
    return composite_entry_to_mongo_doc(entry, now)


def level_pricing_to_mongo_doc(rule: LevelPricingRule, now: int) -> dict[str, Any]:
    return {
        "_id": f"level_pricing:{rule.level_id}",
        "indicator_type": "level_pricing",
        "level_id": rule.level_id,
        "indicator_ref": rule.indicator_ref,
        "offset_type": rule.offset_type,
        "offset_value": rule.offset_value,
        "created_at": now,
        "updated_at": now,
    }


@dataclass(frozen=True)
class MongoSyncCounts:
    indicators: int
    level_pricing: int

    @property
    def total(self) -> int:
        return self.indicators + self.level_pricing


def sync_to_mongo(file: IndicatorsFile, settings: MongoSettings) -> MongoSyncCounts:
    """Upsert indicators and level_pricing into MongoDB."""
    try:
        from pymongo import MongoClient
    except ImportError as e:
        raise ImportError("pymongo is required for Mongo sync: uv add pymongo") from e

    now = int(time.time())
    client = MongoClient(settings.connection_string, serverSelectionTimeoutMS=5000)
    try:
        client.admin.command("ping")
        collection = client[settings.database][INDICATORS_COLLECTION]
        indicator_writes = 0
        for entry in file.indicators:
            doc = entry_to_mongo_doc(entry, now)
            indicator_id = doc["_id"]
            collection.update_one(
                {"_id": indicator_id},
                {
                    "$set": {k: v for k, v in doc.items() if k != "created_at"},
                    "$setOnInsert": {"created_at": doc["created_at"]},
                },
                upsert=True,
            )
            indicator_writes += 1
        pricing_writes = 0
        for rule in file.level_pricing:
            doc = level_pricing_to_mongo_doc(rule, now)
            collection.update_one(
                {"_id": doc["_id"]},
                {
                    "$set": {k: v for k, v in doc.items() if k != "created_at"},
                    "$setOnInsert": {"created_at": doc["created_at"]},
                },
                upsert=True,
            )
            pricing_writes += 1
        return MongoSyncCounts(
            indicators=indicator_writes,
            level_pricing=pricing_writes,
        )
    finally:
        client.close()
