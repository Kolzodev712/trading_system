# Proposal: Indicator Config Generator

**Version**: 1.0  
**Date**: December 2024  
**Status**: Draft

## Executive Summary

This proposal outlines the design and implementation of an **Indicator Config Generator** system that enables developers and traders to define technical indicators using Python scripts, which are then compiled to JSON configurations at build time. This provides a flexible, maintainable approach to indicator configuration while keeping the Rust runtime dependency-free.

The generator supports both simple indicators (SMA, EMA) and composite indicators (built from other indicators using arithmetic operations, functions, and conditionals). It generates system-wide indicator configurations that are available to all strategies, levels, and trading components.

## Objectives

1. **Provide Python DSL for indicator definition** - Allow indicators to be defined in Python with a fluent, readable syntax
2. **Generate JSON configurations at build time** - Compile Python definitions to JSON for Rust to load
3. **Support composite indicators** - Enable building complex indicators from simple ones via expressions
4. **Validate indicator configurations** - Ensure all dependencies exist and configurations are valid
5. **Separate build-time from runtime** - Python used only at build time, Rust loads JSON at runtime
6. **Support dual indicator sources** - System-wide indicators (config) + strategy-specific indicators

## Key Design Principles

1. **Build-Time Generation**: Indicator definitions are compiled at build time, not runtime
2. **No Runtime Python Dependency**: Rust system loads JSON directly, no Python interpreter needed
3. **JSON as Source of Truth**: JSON files are version-controlled and serve as the canonical format
4. **Flexible Indicator Composition**: Support arithmetic, functions, conditionals for composite indicators
5. **Validation Before Generation**: Validate Python definitions before exporting to JSON
6. **Extensibility**: Easy to add new indicator types and expression operations

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│ Python Workspace (python/)                                  │
│  Managed with uv (like Cargo for Rust)                      │
│                                                             │
│  indicator_config_generator/                                │
│  ├── pyproject.toml      # Package dependencies             │
│  ├── src/                                                   │
│  │   ├── builder.py      # Indicator builder DSL            │
│  │   ├── strategies.py   # Pre-built indicator strategies   │
│  │   ├── generator.py    # Main generation script           │
│  │   └── validators.py   # Validation logic                 │
│  └── README.md                                               │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ Generate at build time
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Configuration Files (JSON)                                 │
│                                                             │
│  config/                                                   │
│  └── indicators.json      (version-controlled)             │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ Load at startup
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Rust System (Runtime)                                      │
│                                                             │
│  indicator_engine/                                         │
│  └── Loads and registers indicators from JSON              │
│                                                             │
│  strategy_engine/                                          │
│  └── Merges system + strategy-specific indicators          │
└─────────────────────────────────────────────────────────────┘
```

## Python DSL Design

### Simple Indicators

```python
from indicator_builder import *

# Simple Moving Average
sma_20 = SimpleIndicator.sma("BTCUSDT", period=20, source="close")

# Exponential Moving Average
ema_50 = SimpleIndicator.ema("BTCUSDT", period=50, source="close")
```

### Composite Indicators

```python
# Golden Cross: EMA50 - EMA200
golden_cross = CompositeIndicator(
    id="golden_cross_btcusdt",
    name="Golden Cross",
    pair="BTCUSDT",
    expression=ref("ema_50_btcusdt") - ref("ema_200_btcusdt"),
    dependencies=["ema_50_btcusdt", "ema_200_btcusdt"]
)

# MACD Signal (simplified)
macd_signal = CompositeIndicator(
    id="macd_signal_btcusdt",
    name="MACD Signal",
    pair="BTCUSDT",
    expression=if_then_else(
        ref("ema_12_btcusdt") - ref("ema_26_btcusdt") > 0,
        constant(1.0),
        constant(0.0)
    ),
    dependencies=["ema_12_btcusdt", "ema_26_btcusdt"]
)
```

### Batch Generation

```python
# Generate indicators for multiple pairs
pairs = ["BTCUSDT", "ETHUSDT", "SOLUSDT"]

indicators = []
for pair in pairs:
    indicators.extend([
        SimpleIndicator.sma(pair, 20),
        SimpleIndicator.ema(pair, 50),
        SimpleIndicator.ema(pair, 200),
        golden_cross_for_pair(pair),  # Composite
    ])

# Export to JSON
export_to_json(indicators, "config/indicators.json")
```

## Expression Language

### Supported Operations

- **Arithmetic**: `+`, `-`, `*`, `/`
- **Functions**: `min()`, `max()`, `abs()`, `sqrt()`
- **Conditionals**: `if_then_else(condition, true_expr, false_expr)`
- **References**: `ref(indicator_id)` - reference to another indicator
- **Constants**: `constant(value)` - literal numeric values

### Expression Examples

```python
# (EMA50 - EMA200) * 2 + 10
expression = (ref("ema_50") - ref("ema_200")) * constant(2) + constant(10)

# Maximum of two indicators
expression = max(ref("sma_20"), ref("ema_50"))

# Conditional: if EMA12 > EMA26 then 1 else 0
expression = if_then_else(
    ref("ema_12") > ref("ema_26"),
    constant(1.0),
    constant(0.0)
)
```

## JSON Output Format

### Simple Indicator

```json
{
  "id": "sma_20_btcusdt",
  "name": "SMA",
  "pair": "BTCUSDT",
  "period": 20,
  "source": "close"
}
```

### Composite Indicator

```json
{
  "id": "golden_cross_btcusdt",
  "name": "Golden Cross",
  "pair": "BTCUSDT",
  "type": "composite",
  "expression": {
    "type": "subtract",
    "left": {
      "type": "simple",
      "indicator_id": "ema_50_btcusdt"
    },
    "right": {
      "type": "simple",
      "indicator_id": "ema_200_btcusdt"
    }
  },
  "dependencies": ["ema_50_btcusdt", "ema_200_btcusdt"]
}
```

## Validation Rules

1. **Unique IDs**: All indicator IDs must be unique across the configuration
2. **Dependency Resolution**: All referenced indicators (dependencies) must exist
3. **No Circular Dependencies**: Composite indicators cannot create dependency cycles
4. **Valid Periods**: Period must be > 0 for simple indicators
5. **Valid Sources**: Price source must be valid (close, open, high, low, volume, etc.)
6. **Pair Consistency**: All indicators for a composite must reference the same pair

## Phase 1: Python Builder Library

### 1.1 Core Builder Classes

**Location**: `python/indicator_config_generator/` (Python workspace)

```
python/
├── pyproject.toml          # Workspace root (like Cargo.toml)
├── uv.lock                # Lock file (like Cargo.lock)
└── indicator_config_generator/
    ├── pyproject.toml      # Package dependencies
    ├── README.md
    └── src/
        ├── __init__.py
        ├── builder.py      # Core builder classes
        ├── expressions.py  # Expression language
        ├── validators.py   # Validation logic
        └── exporters.py    # JSON export
```

**Components**:

- `SimpleIndicator` class: Builder for SMA, EMA, etc.
- `CompositeIndicator` class: Builder for composite indicators
- `ExpressionBuilder`: Fluent API for building expressions
- `IndicatorConfigGenerator`: Main generator class

### 1.2 Expression Language Implementation

- Support arithmetic operations
- Support functions (min, max, abs, sqrt)
- Support conditionals (if-then-else)
- Support references to other indicators
- Support constant values

### 1.3 Validation System

- Validate indicator IDs are unique
- Validate dependencies exist
- Detect circular dependencies
- Validate indicator configurations
- Provide helpful error messages

### 1.4 JSON Export

- Serialize indicators to JSON format
- Format output for readability
- Support incremental updates (merge with existing)
- Support validation before export

## Phase 2: Pre-built Indicator Strategies

### 2.1 Common Indicators

Provide pre-built indicator definitions:

- **Moving Averages**: SMA, EMA variants
- **Golden Cross / Death Cross**: EMA50 vs EMA200
- **MACD**: MACD line and signal line
- **RSI-like**: Relative Strength Index (simplified)
- **Bollinger Bands**: Upper, middle, lower bands

### 2.2 Template System

Enable batch generation for multiple pairs:

```python
def generate_indicators_for_pairs(pairs: List[str]) -> List[Indicator]:
    """Generate standard indicators for all pairs"""
    # Template-based generation
```

### 2.3 Reusable Helpers

- Helper functions for common patterns
- Strategy-specific indicator generators
- Pair-specific customizations

## Phase 3: CLI Tool

### 3.1 Command-Line Interface

```bash
# Navigate to Python workspace
cd python

# Install dependencies (first time)
uv sync

# Generate indicators from Python script
uv run indicator-generate \
    --pairs BTCUSDT ETHUSDT \
    --output ../config/indicators.json \
    --validate

# Validate existing config
uv run indicator-validate \
    --config ../config/indicators.json

# Or use as module
uv run python -m indicator_config_generator.cli generate \
    --pairs BTCUSDT ETHUSDT \
    --output ../config/indicators.json
```

### 3.2 Integration with Build System

- Pre-build script to generate indicators
- Validation in CI/CD
- Error handling and reporting

## Phase 4: Rust Integration

### 4.1 JSON Loading in Indicator Engine

**Location**: `src/indicator_engine/`

- Extend `IndicatorService` to load from JSON
- Validate JSON structure matches Rust types
- Register indicators from JSON config

### 4.2 Validation on Load

- Validate all dependencies exist
- Check for duplicate IDs
- Verify configuration completeness
- Handle missing dependencies gracefully

### 4.3 Integration with Strategy Engine

- Strategy Engine extracts strategy-specific indicators
- Merge with system-wide indicators
- Coordinate registration with Indicator Engine

## Integration Points

### With Indicator Engine

- Indicator Engine loads `config/indicators.json` at startup
- All indicators registered before strategies load
- Indicator Engine handles calculation and updates

### With Strategy Engine

- Strategies can reference system indicators by ID
- Strategies can define additional indicators in their IR
- Strategy Engine coordinates indicator registration

### With Build System

- Python generation runs during build/deploy
- Generated JSON committed to repository or deployed
- Validation step ensures correctness

## Error Handling

### Build-Time Errors

- Invalid Python syntax → Clear error message
- Missing dependencies → List all missing indicators
- Circular dependencies → Show dependency chain
- Invalid configurations → Specific validation errors

### Runtime Errors

- Missing JSON file → Graceful degradation (load from MongoDB or use defaults)
- Invalid JSON structure → Error log, skip invalid indicators
- Missing dependencies → Log warning, indicator not ready

## Testing Strategy

### Python Layer Tests

- Unit tests for builder classes
- Unit tests for expression evaluation
- Integration tests for full generation workflow
- Validation test cases

### Rust Integration Tests

- Load generated JSON successfully
- Handle missing dependencies
- Validate indicator registration
- Test composite indicator calculation

### End-to-End Tests

- Generate indicators in Python
- Load in Rust
- Verify indicators calculate correctly
- Test composite indicator dependencies

## Performance Considerations

### Build Time

- Python generation is fast (< 1 second for typical configs)
- JSON serialization is efficient
- Validation runs in-memory

### Runtime

- JSON parsing is fast (Rust serde_json)
- Indicator registration is O(n) where n = number of indicators
- No performance impact on indicator calculation (same as hand-written configs)

## Future Enhancements

1. **Visual Indicator Builder**: GUI for building indicators
2. **Indicator Backtesting**: Test indicators on historical data
3. **Indicator Optimization**: Auto-tune parameters
4. **Indicator Marketplace**: Share indicator definitions
5. **Real-time Validation**: Validate indicators during development
6. **Binary Format**: Optional binary serialization for faster loading (if needed)

## Migration Path

### Existing Hand-Written Configs

1. Manually convert existing configs to Python scripts
2. Generate JSON from Python
3. Verify output matches existing configs
4. Switch to Python-generated configs
5. Remove hand-written configs

### Gradual Adoption

- Support both Python-generated and hand-written configs
- Gradually migrate to Python generation
- Keep Python generation optional initially

## Dependencies

### Python Dependencies

Managed via `uv` and `pyproject.toml` in the Python workspace:

- `pydantic>=2.0.0`: Type validation and data modeling
- `pydantic-settings>=2.0.0`: Settings management
- Standard library: JSON serialization (built-in)

**Location**: `python/indicator_config_generator/pyproject.toml`

**Management**:
```bash
cd python
uv add pydantic  # Add dependency
uv sync          # Install dependencies
```

### Rust Dependencies

- `serde_json`: JSON parsing (already in use)
- `indicator_engine`: Indicator registration (existing)

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Python syntax changes break configs | High | Version pinning, comprehensive tests |
| Generated JSON doesn't match Rust types | High | Validation, type checking in CI |
| Circular dependencies not caught | Medium | Graph-based validation |
| Performance issues with large configs | Low | Optimize serialization, consider binary format |

## Success Criteria

1. ✅ Python DSL is intuitive and easy to use
2. ✅ Generated JSON loads correctly in Rust
3. ✅ All indicator types (simple + composite) work correctly
4. ✅ Validation catches all invalid configurations
5. ✅ Build-time generation adds < 5 seconds to build
6. ✅ Runtime JSON loading is fast (< 100ms for typical configs)

---

**Next Steps**: 
1. ✅ Python workspace structure created (`python/` with `uv`)
2. Implement basic builder classes
3. Create expression language
4. Implement validation
5. Integrate with Rust indicator_engine

**Note**: The Python workspace is set up using `uv` for dependency management (similar to Cargo for Rust). See `python/README.md` and `python/SETUP.md` for setup instructions.

