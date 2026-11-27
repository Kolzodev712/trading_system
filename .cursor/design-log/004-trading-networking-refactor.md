# Trading networking refactor assessment

## Problem / context

`libs/trading_networking` is the **optional external data plane** (wire types, buses, Zenoh gateway, remote commands). Internal trading truth stays in `trading_core` broadcast channels; `runner::event_bridge` maps into `HotEvent` / `ColdEvent` / `DexColdEvent` when `ZENOH_ENABLED=1`. Assessment requested after platform readiness (design log 003): does the crate still earn its place?

## Refactor assessment

### Scope

- **Crate:** `trading_networking` (~22 modules): `events/`, `payload/`, `commands/`, `gateway/` (EventGateway, backpressure, transports), optional `zenoh_bridge`.
- **Sole in-repo consumer:** `runner` (feature `zenoh`). **Not** used by `gateway` (gRPC/TUI path is separate).
- **Runtime default:** Zenoh off; bridge tasks still run but gateway never starts without env.

### Systems / tech

| Aspect | Today | Gap |
|--------|--------|-----|
| **Lifecycle** | Started with runner; stops when process exits | No graceful Zenoh session teardown doc |
| **Coupling** | Duplicates `trading_core` MD/CEX/DEX shapes; mapping only in `event_bridge` | Schema drift risk (two enums to maintain) |
| **Observability** | `TransportHealth` per gateway slot; not wired to runner `/health` | External lane health invisible to ops |
| **Security** | Zenoh default open config; no auth in crate | Prod needs keyexpr ACL / TLS story |
| **Ops cost** | Heavy optional `zenoh` dep (~1.0) | Compile time + binary size when feature on |
| **Consumers** | No in-repo subscriber; export for external tools | Value unproven without a real consumer |

### Rust / API quality

- **Strengths:** Clear split hot MPSC vs cold/dex broadcast; `EventTransport` trait + one Zenoh impl; JSON payload roundtrip tests; `CommandRouter` trait object boundary for Zenoh queryable.
- **Dead-ish surface:** `HotEventBus` trait (only `HotEventTx` impl, unused externally); `HotEventBus` re-export vs actual API `hot_event_channel`; `DtpError` alias barely used; lib docs still say “later phase” for gateway.
- **Commands:** `CommandPayload` mirrors gRPC but level CRUD returns “not implemented” in `RunnerCommandHandler`; **duplicate control plane** with `gateway` gRPC (TUI uses gRPC per docs).
- **Errors:** `NetworkError` + string errors on transport; adequate for opt-in export, not for library consumers yet.

### Usefulness verdict

**Keep with internal refactor** — not remove, not merge into `trading_core`, not replace with a different crate yet.

The crate **earns its place** as the **wire-format + transport boundary** for external observability/control, explicitly **not** the internal bus (decision in log 003). Current **value is latent**: correct architecture, **low active use** (Zenoh off, no external subscriber in tree).

## Alternatives

| Option | Pros | Cons | Effort | When |
|--------|------|------|--------|------|
| **A. Keep + tighten (recommended)** | Preserves layering; shrink dead API; document contract; optional metrics hook | Still two event enums | Small–medium | Default path |
| **B. Merge into `runner` as `runner::export`** | One less crate; only one consumer | Blurs “library” story; harder to test wire format in isolation | Medium | If Zenoh never leaves runner |
| **C. Replace Zenoh with dedicated bus (e.g. NATS, Redis streams)** | Different ops profile | New dep, rewrite gateway; no current pain | Large | Multi-datacenter fanout requirement |
| **D. Remove crate; gRPC-only external** | Simplest ops story | Loses pub/sub event export; HFT/control-plane separation weaker | Medium | If event streaming to external tools is abandoned |

## Recommendation (Step 2 if approved)

**Option A — keep + refactor internally:**

1. Trim public API: deprecate/remove unused `HotEventBus` trait export or implement for real; update crate docs.
2. Move `event_bridge` mapping helpers next to wire types (or `From` impls) to reduce drift vs `trading_core` channels.
3. Wire `EventGateway` transport health into runner metrics or log on interval when Zenoh enabled.
4. Document command plane: Zenoh = automation/remote; gRPC = TUI; which `CommandPayload` variants are supported.
5. Spike only if adding a real consumer (dashboard, recorder): one week, measure lag/CPU with `ZENOH_ENABLED=1`.

**Do not** route internal trading through this crate (invariant from 003).

### Acceptance criteria (success)

- No duplicate bridge logic outside `event_bridge`.
- Crate docs match PLATFORM_DATA_PLANE.md.
- With `ZENOH_ENABLED=1`, hot/cold/dex events publish without panic; transport health observable.
- `cargo test -p trading_networking` + `just check-venue-imports` green; runner unchanged on default path.

### Abandon signals

- No external consumer after 6 months and Zenoh never enabled in any env → reconsider **B** or **D**.
- Zenoh operational cost exceeds benefit → **C** or gRPC-only events via streaming RPC.

## Decision

**Verdict:** **Keep with minor refactor** (Option A). Do not remove; do not make internal bus.

## Alternatives

(See table above — preferred: Option A.)

## Consequences

- Continued maintenance of parallel wire types vs `trading_core` until `From`/`TryFrom` or codegen links them.
- `zenoh` feature remains optional; workspace default build stays lean.
- Command duplication with gRPC remains until Zenoh command surface is narrowed or completed.

## Verification

- Before/after: `cargo build -p runner` with and without `zenoh` feature; compile time delta.
- Runtime: `ZENOH_ENABLED=1` smoke — subscribe with `zenoh-cli` to `trading/hot/*` and `trading/dex_cold/*`; one cold + one hot message received.
- Static: `rg trading_networking` — expect only `runner` in `src/`.
- Tests: existing payload roundtrips + cold bus tests; add transport health unit test if refactored.

## Implementation results

**Shipped (Option A — all assessment items):**

1. **API trim** — Removed public `HotEventBus` trait and `DtpError` re-export; crate docs updated to describe gateway + conversion helpers.
2. **Conversion** — Added `events/convert.rs` with `hot_event_from_market_data`, `cold_event_from_cex`, `dex_cold_event_from_dex`; `runner::event_bridge` delegates to these (no inline mapping).
3. **Transport health** — `maybe_start_zenoh_gateway` polls `TransportHealth` every 10s; `runner::zenoh_metrics` atomics; Prometheus `zenoh_transport_*` in `platform_metrics`.
4. **Docs** — `docs/COMMAND_PLANE.md` (Zenoh vs gRPC, supported payloads); `docs/ZENOH_SMOKE.md`; `PLATFORM_DATA_PLANE.md` cross-links.
5. **Spike** — Documented Zenoh subscribe smoke (no new binary).

**Deviations:** None.

**Verification:** `cargo fmt --all`; `cargo test -p trading_networking --all-features` (12 passed); `cargo clippy -p trading_networking -p runner --lib -D warnings` (clean); `cargo test --all-features` (workspace green); `bash .cursor/tools/check_venue_imports.sh` OK.

[cursor-hub workflow] step=refactor-assessment

[cursor-hub workflow] step=refactor-implement
