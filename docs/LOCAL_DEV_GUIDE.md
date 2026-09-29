# Local development: k-paper + trading system + legacy CLI smoke

This guide runs the existing **Spot/levels connectivity smoke test**: the **k-paper** exchange, the **trading system runner**, and the old **TUI CLI**. It does not qualify the Candidate #2 futures or fault-injection stages.

Architecture: [ARCHITECTURE.md](ARCHITECTURE.md). Increment status: [PROGRESSION.md](PROGRESSION.md). Paper server: [K-paper integration guide](https://github.com/Kolzodev712/k-paper/blob/master/docs/PROP_TRADING_INTEGRATION_GUIDE.md). Browser UI: [Trading System UI control-plane guide](https://github.com/Kolzodev712/trading-system-ui/blob/main/docs/PROP_TRADING_CONTROL_PLANE_GUIDE.md).

Use **three terminals**. Start services in order: k-paper → runner → CLI.

## What talks to what

| Component | Repo / path | Port / endpoint | Role |
|-----------|-------------|-----------------|------|
| **k-paper** | `~/Desktop/k-paper` (separate repo) | HTTP/WS on `127.0.0.1:<port>` | Mock exchange: balances, orders, market data |
| **Runner** | `trading_system` | gRPC `http://127.0.0.1:50051` | Trading engine, indicators, levels |
| **CLI** | `trading_system` (`cli` package) | Connects to runner gRPC | TUI monitor / control |

The CLI does **not** connect to k-paper. The runner connects to k-paper using `config/bot.json` → `paper_exchange_url`.

**Health HTTP** (runner): `bot.json` → `server.port` (e.g. `8080`) — not used by the CLI.

- `GET /health` — returns **503** when CEX-critical dependencies are down (`venue.mode` `cex` or `both`). Body includes `detail` with readiness breakdown.
- `GET /metrics` — Prometheus text when trace exporters enable Prometheus; includes `venue_mode`, `dex_execute_*`, `broadcast_lag_total` counters.
- **Zenoh** (optional): set `ZENOH_ENABLED=1` to export hot/cold/dex-cold events. Default is off. See [Platform data plane](PLATFORM_DATA_PLANE.md) and [Venue modes](VENUE_MODES.md).

## Prerequisites

- Rust toolchain (`cargo`)
- **MongoDB** on `mongodb://localhost:27017` if `config/bot.json` has `"mongo": { "enabled": true }` (default in k-paper sample config)
- Config files under `trading_system/config/`:
  - `bot.json` — exchange + pairs + mongo
  - `levels.json` — level templates (`"price": "Binding"`)
  - `indicators.json` — indicators + `level_pricing` (see [Levels Guide](LEVELS_GUIDE.md))

Optional: Python `uv` only if you regenerate `indicators.json` (see `python/README.md`).

## One-time config setup

From `trading_system` root:

```bash
cp sample_config/k-paper/bot.json config/bot.json
cp sample_config/k-paper/levels.json config/levels.json
cp sample_config/k-paper/indicators.json config/indicators.json
```

Edit `config/bot.json` so **`paper_exchange_url` uses the same host/port as k-paper** (see below). Default recommendation: port **19999**.

## Paper port and market feed

Current K-paper `runtime.rs` defaults to fixed port **19999** if `PAPER_EXCHANGE_PORT` is unset. Set it explicitly for this smoke test so the server and `bot.json` visibly agree.

**k-paper:**

```bash
export PAPER_EXCHANGE_PORT=19999
export PAPER_EXCHANGE_PROFILE=dev
```

**`config/bot.json`** (exchange section):

```json
"paper_exchange_url": "ws://127.0.0.1:19999/ws/trade"
```

Use path **`/ws/trade`** for the runner (orders + account). Market data uses `/ws/market` on the same host/port automatically.

---

## Terminal 1 — k-paper

```bash
cd ~/Desktop/k-paper
PAPER_EXCHANGE_PORT=19999 PAPER_EXCHANGE_PROFILE=dev cargo run
```

**Success lines:**

```text
profile=Dev scenario=None market_engine=true instruments=5
paper exchange listening on 127.0.0.1:19999
```

**Sanity check** (another shell):

```bash
curl -s http://127.0.0.1:19999/balances | head
curl -s http://127.0.0.1:19999/exchange_info | head
```

Leave this terminal running.

### Market feed notes

- The checked-in `config/instruments.yaml` has a `market` section with BTC, ETH, SOL, BNB and XRP. Current `runtime.rs` can start the market engine from this section even without the legacy `PAPER_EXCHANGE_DUMMY_OB=1` flag; `PAPER_EXCHANGE_PROFILE=dev` makes interactive intent explicit.
- A **static** `PAPER_EXCHANGE_SCENARIO` without the legacy dummy flag suppresses the random market engine. Verify the `market_engine=...` startup log instead of inferring feed behavior solely from the profile name.
- After starting the runner, wait **~15–20 seconds** for SMA(20) to warm up before expecting indicator values.

---

## Terminal 2 — trading system runner

```bash
cd ~/Desktop/trading_system
cargo run --release --package runner --bin runner -- --mode levels
```

(`--bin runner` is required: the package also builds `test_runner`.)

**Success lines (among others):**

```text
k-paper: connected to ws://127.0.0.1:19999/ws/trade
k-paper market data: connected to ws://127.0.0.1:19999/ws/market
gRPC server started on [::]:50051
```

**Verify gRPC:**

```bash
ss -tlnp | grep 50051
```

Leave this terminal running.

### Optional: regenerate indicators

Only if you changed pairs or templates:

```bash
cd python && uv sync
uv run indicator-generate --pairs BTCUSDT,ETHUSDT --output ../config/indicators.json --no-mongo
uv run indicator-validate --config ../config/indicators.json
```

---

## Terminal 3 — TUI CLI

After the runner logs **gRPC server started**:

```bash
cd ~/Desktop/trading_system
cargo run --release --package cli -- http://127.0.0.1:50051
```

Default endpoint is the same if you omit the URL:

```bash
cargo run --release --package cli
```

**CLI tips:** `Q` quit, `Tab` switch panels, spot monitor shows orderbooks / indicators / levels. Debug log: `cli_debug.log` in the repo root.

---

## Quick reference (copy-paste)

```bash
# 1) k-paper
cd ~/Desktop/k-paper
PAPER_EXCHANGE_PORT=19999 PAPER_EXCHANGE_PROFILE=dev cargo run

# 2) runner (new terminal)
cd ~/Desktop/trading_system
cargo run --release --package runner --bin runner -- --mode levels

# 3) CLI (new terminal, after gRPC is up)
cd ~/Desktop/trading_system
cargo run --release --package cli -- http://127.0.0.1:50051
```

---

## Troubleshooting

### `Address already in use` when starting k-paper

An old k-paper is still bound to the port.

```bash
ss -tlnp | grep 19999    # or 33223 if you use that port
kill -9 <k-paper-pid>
```

Stop the **runner** first if it still holds connections to the old server (`kill <runner-pid>`), then kill k-paper, start it with the intended profile/scenario, and restart the runner.

### CLI connection error / nothing on port 50051

- Runner not running, or stuck before gRPC (check runner logs for errors).
- Wrong target: CLI must use **50051**, not k-paper’s port and not **8080**.
- Start CLI **after** `gRPC server started on [::]:50051`.

### Runner: `Connection refused` to k-paper

- k-paper not running, or wrong port in `config/bot.json` vs `PAPER_EXCHANGE_PORT`.
- Fix URL: `ws://127.0.0.1:<port>/ws/trade`.

### No orderbooks / no indicators in CLI

- Check K-paper's `market_engine=...` startup log and `config/instruments.yaml`; a static lab scenario may provide no continuing market feed.
- Confirm the pair is configured in both `bot.json` and K-paper's instruments/market settings.
- Wait ~15–20s after runner start for indicator warmup.

### Jaeger / OpenTelemetry warnings

Harmless if you are not running Jaeger (`localhost:6831`). Runner continues; ignore or run a Jaeger agent.

### MongoDB

If runner fails on Mongo connect: start Mongo (`mongod` or `docker compose up -d mongodb` — compose may map **27018** on host; align `connection_string` in `bot.json`), or set `"mongo": { "enabled": false }` for a file-only trial.

---

## Related docs

- [Levels Guide](LEVELS_GUIDE.md) — `Binding` price + `indicators.json` / `level_pricing`
- [python/README.md](../python/README.md) — indicator config generator
- [README.md](../README.md) — full project overview
