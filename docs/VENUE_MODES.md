# Venue modes

Configured in `config/bot.json` under `venue.mode`.

| `venue.mode` | CEX engine | DEX engine | Typical `--mode` |
|--------------|------------|------------|------------------|
| `cex` | Yes | No | `levels`, `strategies` |
| `dex` | No (MD optional stub) | Yes | `dex` |
| `both` | Yes | Yes | `dex` (DEX strategies) or `levels`/`strategies` (CEX only — DEX runs for monitoring) |

## Semantics

- **`cex`:** Production default. Levels/strategies require this mode.
- **`dex`:** DEX strategy engine only; mock chain until Solana (Proposal 01).
- **`both`:** Both stacks in one process. CEX trading paths unchanged; DEX state appears in gRPC `MonitoringData.dex`. Not coordinated cross-venue hedging.

## Examples

- `config/bot.json` — `cex` + Paper/Binance
- `config/bot.dex.example.json` — `dex`
- `config/bot.both.example.json` — `both`

## Health

- `cex` / `both`: `/health` returns **503** when CEX bootstrap or market data freshness fails (see `runner::platform_health`).
- `dex` / `both`: DEX mock provider must be bootstrapped (`dex_ready`).
