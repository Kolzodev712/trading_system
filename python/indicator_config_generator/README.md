# Indicator Config Generator

**Status**: Implemented

## Overview

Build-time Python tooling that writes `config/indicators.json` (and optionally syncs to MongoDB). The Rust `indicator_engine` loads indicators and `level_pricing` at runtime; levels use `"price": "Binding"` and resolve pricing via `level_pricing` keyed by level template `id`.

## Quick start

From the repo:

```bash
cd python
uv sync
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json
uv run indicator-validate --config ../config/indicators.json
```

Optional Mongo sync (uses `mongo` settings from `config/bot.json` when present):

```bash
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json --sync-mongo
```

**CLI:** `--pairs`, `--output`, `--templates`, Mongo flags.

**Templates** (`--templates`, comma-separated):

| Template | Adds per pair |
|----------|----------------|
| `default` | SMA 20, EMA 12 (committed `config/indicators.json`) |
| `composites` | EMA 50/200, golden cross, death cross |
| `macd` | EMA 12/26, MACD line composite |

Example: `--templates default,composites,macd`. Level pricing defaults are in `builder.py` (`buy_level_1` / `sell_level_1` → `sma_20` ±0.5%).

**CI / local:** from repo root, `just indicator-validate`, `just indicator-config-check`.

## Output format

```json
{
  "version": 1,
  "indicators": [
    {
      "kind": "simple",
      "id": "sma_20_BTCUSDT",
      "name": "SMA",
      "pair": "BTCUSDT",
      "period": 20,
      "source": "close"
    }
  ],
  "level_pricing": [
    {
      "level_id": "buy_level_1",
      "indicator_ref": "sma_20",
      "offset_type": "PercentBelow",
      "offset_value": "0.5"
    }
  ]
}
```

- **Indicator ids**: `{type}_{period}_{pair}` (e.g. `sma_20_BTCUSDT`)
- **`level_pricing.indicator_ref`**: pair-agnostic (`sma_20`) or full id; resolved per pair at runtime
- **Composites**: `kind: "composite"` with expression + dependencies (validated at load)

## Runtime load order (Rust)

1. **MongoDB** `indicators` collection (if enabled): indicators + `level_pricing` documents
2. **`config/indicators.json`**: fills gaps; Mongo wins on duplicate indicator ids
3. If Mongo already has indicators, file registration for indicators is skipped (file `level_pricing` may still apply if Mongo has none)

## Package layout

```
indicator_config_generator/
├── cli.py           # indicator-generate, indicator-validate entrypoints
├── generator.py
├── builder.py       # default templates per --pairs
├── models.py
├── expressions.py
├── validators.py
├── exporters.py
└── mongo.py
```

## Related docs

- [Python workspace README](../README.md)
- [Levels guide](../../docs/LEVELS_GUIDE.md) — `"price": "Binding"` and `level_pricing`
- [Design log 001](../../.cursor/design-log/001-indicator-config-generator.md)
