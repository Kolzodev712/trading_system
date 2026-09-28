# Command plane

Remote control uses two paths. Pick the one that matches your client.

## Zenoh (automation / external tools)

- **Enable:** `ZENOH_ENABLED=1` on the runner process.
- **Transport:** Zenoh queryable on `trading/commands` (see `ZenohKeyspace` in `trading_networking`).
- **Wire format:** JSON `Command` / `CommandResponse` via `trading_networking::payload`.
- **Handler:** `runner::commands::RunnerCommandHandler` (levels/strategies modes) or `DexNoopHandler` (dex-only).

### Supported `CommandPayload` variants (Zenoh)

| Variant | Status |
|---------|--------|
| `SetTradingEnabled` | Supported |
| `GetTradingStatus` | Supported |
| `ToggleStrategy` | Supported (requires strategy manager) |
| `GetLevels`, `GetLevel`, `CreateLevel`, `UpdateLevel`, `DeleteLevel`, `ToggleLevel` | **Not implemented** — returns `"command not implemented"` |

## gRPC (browser UI / TUI / gateway)

- **Service:** `TradingCliService` is implemented in the `gateway` crate and is instantiated by the CEX runner at `:50051`. A separate `gateway` proxy binary can forward from `:50052` to the runner, but it is not required for the normal browser/TUI path.
- **Clients:** the legacy TUI uses native gRPC; `trading-system-ui` uses gRPC-Web through Envoy to the runner. Neither client talks directly to the exchange.
- **Overlap:** Command payloads mirror Zenoh for consistency; level CRUD may differ by gateway version — prefer gRPC for level management until Zenoh level commands are implemented.
- **Prop-trading controls:** `PauseEntries`, `CancelWorking`, `FlattenAndPause` and `Resume` are separate commands, not aliases of `SetTradingEnabled`. Authoritative state is `GetTradingStatus` / `MonitoringData` (`entries_paused`, `flatten_latched`). Mutating RPCs require `x-operator-token` matching `TRADING_OPERATOR_TOKEN` on every non-Paper venue; `TRADING_OPERATOR_ID` is the audited identity. An unset token is Paper/local only. Envoy injects the token from a server secret; the browser bundle must not contain it. UI login and `x-operator-id` are not authorization. See [the implementation roadmap](PROP_TRADING_RESEARCH_AND_PRODUCTION_PLAN.md).

## Invariants

- Commands do **not** flow through the hot/cold event buses.
- Disabling trading via Zenoh uses the same `TradingControl` path as gRPC.
- DEX-only runner (`--mode dex`) accepts Zenoh queries but responds with `dex mode: remote commands not supported`.
