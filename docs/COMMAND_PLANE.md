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

## gRPC (TUI / gateway)

- **Service:** `gateway` crate (separate binary from runner).
- **Client:** TUI and ops tooling use gRPC, not Zenoh.
- **Overlap:** Command payloads mirror Zenoh for consistency; level CRUD may differ by gateway version — prefer gRPC for level management until Zenoh level commands are implemented.

## Invariants

- Commands do **not** flow through the hot/cold event buses.
- Disabling trading via Zenoh uses the same `TradingControl` path as gRPC.
- DEX-only runner (`--mode dex`) accepts Zenoh queries but responds with `dex mode: remote commands not supported`.
