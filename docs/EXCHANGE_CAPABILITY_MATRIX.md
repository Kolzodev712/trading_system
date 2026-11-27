# Exchange capability matrix

**As of:** 2026-11-27  
**Purpose:** Honest, high-level status for venue adapters in `trading_system`. This is not an SLA or production sign-off.

**Legend**

| Label | Meaning |
| --- | --- |
| **Implemented** | Code paths exist in the workspace and compile with default or documented features |
| **Component-tested** | Unit / component tests in this repo exercise the adapter logic (mocks or recorded fixtures) |
| **Paper-verified** | Process workflows (`just test-paper*`) or showcase `lab/` scenarios against **k-paper** |
| **Demo** | Bounded qualification on vendor **testnet/demo** APIs (see [PROGRESSION.md](PROGRESSION.md)) |
| **Experimental** | Behind feature flags, incomplete operator controls, or known gaps |
| **Unsupported** | Not maintained for public use, or explicitly out of scope |

---

## CEX

| Venue / mode | Implemented | Component-tested | Paper-verified | Demo | Notes |
| --- | --- | --- | --- | --- | --- |
| **k-paper** (Spot smoke) | Yes | Yes | Yes (levels smoke) | — | Separate [**k-paper**](https://github.com/Kolzodev712/k-paper) repo; not Bybit parity |
| **k-paper** (USDT-linear Candidate #2) | Yes | Partial | Yes (`just test-paper`) | — | Linear fixture + fault hooks; account lost on Paper restart |
| **Bybit** (Spot) | Yes | Partial | Unknown | Unknown | Adapters in `exchange_engine`; public gate focus has been Demo linear, not spot |
| **Bybit** (USDT-linear perp) | Yes | Partial | Via k-paper only | **Yes** (bounded, synthetic signal) | Increment 10 evidence in repo docs; **not** mainnet |
| **Bybit** (mainnet) | Partial | Partial | — | — | **Unsupported** for public onboarding; operator token + interlocks required |
| **Binance** (Spot) | Yes | Partial | — | — | **Experimental** for new work; less qualification than Bybit/Paper paths |
| **Binance** (Futures) | Partial | Partial | — | — | **Unknown** public qualification depth |

---

## DEX (opt-in: `cargo` feature `dex`)

| Venue / chain | Implemented | Component-tested | Paper-verified | Demo | Notes |
| --- | --- | --- | --- | --- | --- |
| **DeepBook on Sui** (indexer MD) | Yes | Partial | — | **Experimental** | `--mode dex`; testnet indexer; wallet key for execution layers |
| **DeepBook live execution** | Partial | Partial | — | — | Phasing in design logs; **not** part of Paper lab |
| **Cross-venue (`both` mode)** | Yes | Partial | — | — | CEX + DEX monitor in one process; **not** coordinated hedging |

DEX crates (`dex_engine`) are **not** in the default Paper qualification path. See [VENUE_MODES.md](VENUE_MODES.md).

---

## Cross-cutting

| Capability | Status |
| --- | --- |
| Levels + indicators (CEX) | Implemented; Paper-verified for spot levels smoke |
| Built-in Rust strategies (`BuyLowSellHigh`, etc.) | Implemented; maturity varies; distinct from Candidate #2 |
| Candidate #2 (`btc_price_ma_app`) | Implemented composition; Paper-verified; **not** validated alpha |
| Research backtest (`research_cli`) | Implemented; **not** live execution |
| TUI CLI (`cli` package) | Implemented; secondary to browser UI |
| Zenoh data plane | Implemented, **opt-in** (`ZENOH_ENABLED=1`); **unsupported** in public lab |

---

## Unknowns (explicit)

- Relative production readiness of **Binance** vs **Bybit** adapters on mainnet.
- End-to-end **DEX** execution vs indexer-only monitoring on public testnet.
- Latency/throughput numbers for in-process channels (no published benchmark in this matrix).

Update this file when [PROGRESSION.md](PROGRESSION.md) or showcase `lab/` scenario tables change.