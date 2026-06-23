# Venue modes

Configured in `config/bot.json` under `venue.mode`.

| `venue.mode` | CEX engine | DEX engine | Typical `--mode` |
|--------------|------------|------------|------------------|
| `cex` | Yes | No | `levels`, `strategies` |
| `dex` | No | Yes | `dex` |
| `both` | Yes | Yes | `dex` (DEX strategies) or `levels`/`strategies` (CEX only — DEX runs for monitoring) |

## Semantics

- **`cex`:** Production default. Levels/strategies require this mode.
- **`dex`:** DEX strategy engine + DeepBook market data when `dex_engine.venue` is `deepbook` and `chain` is `sui`.
- **`both`:** Both stacks in one process. CEX trading paths unchanged; DEX state appears in gRPC `MonitoringData.dex`. Not coordinated cross-venue hedging.

## Naming: exchange vs chain

| Layer | Example | Config field |
|-------|---------|--------------|
| **Exchange (venue)** | DeepBook V3 CLOB | `dex_engine.venue`: `"deepbook"` |
| **Chain** | Sui | `dex_engine.chain`: `"sui"` |
| **Market data** | DeepBook indexer | `market_data_engine` → `exchanges/deepbook/` |

Legacy `"venue": "sui"` in JSON still deserializes to `deepbook` (serde alias).

## DeepBook on Sui (testnet)

Use `config/bot.dex.example.json` as a template:

- `dex_engine.venue`: `"deepbook"` (exchange)
- `dex_engine.chain`: `"sui"` (chain)
- `dex_engine.config.pools`: indexer pool names (testnet: `SUI_DBUSDC`)
- Indexer URL defaults from `network` (`testnet` → `https://deepbook-indexer.testnet.mystenlabs.com`)
- `SUI_PRIVATE_KEY` env var required only from Layer 2+ (wallet reads / execution)

Run:

```bash
cargo run --release -p runner -- --mode dex --config config/bot.dex.example.json
```

(`--config` / `-c` selects the bot JSON; defaults to `config/bot.json`.)

Live orderbooks/trades appear in `MonitoringData` via DeepBook indexer polling (~1 Hz).

## Examples

- `config/bot.json` — `cex` + Bybit/Paper
- `config/bot.dex.example.json` — `dex` + DeepBook on Sui testnet
- `config/bot.both.example.json` — `both`

## Health

- `cex` / `both`: `/health` returns **503** when CEX bootstrap or market data freshness fails (see `runner::platform_health`).
- `dex` / `both`: DeepBook provider must bootstrap (`dex_ready`); loads pool metadata from indexer at startup.

See design log [006-deepbook-sui-phasing](../.cursor/design-log/006-deepbook-sui-phasing.md) for phasing.
