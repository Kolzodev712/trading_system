# Running the Trading System CLI

## Quick Start

### 1. Start the Runner (Trading System)

The runner starts the gRPC server that the CLI connects to.

**For Levels Mode:**
```bash
cargo run --package runner -- --mode levels --levels-config config/levels.json
```

**For Strategies Mode:**
```bash
cargo run --package runner -- --mode strategies --strategies-config config/strategies.json
```

The gRPC server will start on `[::1]:50051` (localhost IPv6) or `127.0.0.1:50051` (localhost IPv4).

### 2. Start the CLI (Terminal User Interface)

In a **separate terminal**, run:

```bash
cargo run --package cli -- http://[::1]:50051
```

Or if IPv6 doesn't work, use IPv4:
```bash
cargo run --package cli -- http://127.0.0.1:50051
```

### 3. Using the CLI

Once the CLI starts, you'll see the Main Menu with:

- **Trading Status**: Shows if trading is enabled/disabled
- **Navigation Options**:
  - `[1]` - Trading Monitor (not yet implemented)
  - `[2]` - System Monitor (not yet implemented)  
  - `[3]` - Live Config (not yet implemented)
  - `[T]` - Toggle trading on/off
  - `[Q]` - Quit

**Press `T` to toggle trading enabled/disabled** - when disabled, all active orders will be cancelled automatically.

**Press `Q` to quit the CLI** (the runner will continue running).

## Troubleshooting

### Connection Issues

If you get a connection error:
1. Make sure the runner is running first
2. Check that the gRPC server started (look for "gRPC server started on ..." in runner logs)
3. Try using `127.0.0.1:50051` instead of `[::1]:50051`

### Port Already in Use

If port 50051 is already in use, you can change it in `src/runner/src/lib.rs`:
- Search for `"[::1]:50051"` and change to a different port
- Update the CLI command to use the same port

## Notes

- The runner must be running **before** starting the CLI
- Closing the CLI won't stop the runner (they're separate processes)
- Use Ctrl+C in the runner terminal to stop the trading system
- The CLI is read-only for monitoring; trading control (enable/disable) is available

