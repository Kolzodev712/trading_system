# Workload and platform facts

## Performance importance

Performance materially important: only certain components

## Workload facts

Deployment CPU architecture: x86_64
Production CPU family: UNKNOWN
Operating system: Linux
Rust/toolchain version constraints: rust-toolchain.toml pin (1.98.x), edition 2021
Typical event/request rate: UNKNOWN
Peak event/request rate: UNKNOWN
Typical active cardinality: low tens of instruments (Paper/Demo)
Maximum expected cardinality: UNKNOWN
Read/write balance: write-heavy on market-data path; read-heavy on gateway monitoring
Burst behavior: venue WS bursts; reconnect storms
Latency target: UNKNOWN
Tail-latency target: UNKNOWN
Memory constraint: UNKNOWN
Threading/runtime model: Tokio multi-thread; broadcast fan-out to engines
Known allocation constraints: prefer Arc snapshots on orderbook fan-out; avoid deep clones per tick
Deployment topology: single host / docker-compose; Paper process for labs

<!-- doctrine-setup:workload -->
