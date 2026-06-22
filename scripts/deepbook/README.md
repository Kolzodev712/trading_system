# DeepBook setup utilities

Tools for one-off Sui / DeepBook setup. The trading bot does **not** require funded wallets until you execute on-chain commands.

## Create BalanceManager

Creates, registers, and shares a DeepBook V3 `BalanceManager` (single PTB). Prints `balanceManagerId` to paste into `dex_engine.config`.

**When you need it:** before placing orders or depositing into DeepBook. Not required for read-only mode (orderbook + wallet balance polling).

**Requires:** `SUI_PRIVATE_KEY` and a small amount of SUI on the chosen network for gas.

```bash
export SUI_PRIVATE_KEY='suiprivkey1...'   # or 64-char hex

# Default config: config/bot.dex.example.json
./scripts/deepbook/create-balance-manager.sh

# Or specify bot config (network + package ids come from dex_engine.config):
./scripts/deepbook/create-balance-manager.sh config/bot.dex.local.json
```

Equivalent direct invocation:

```bash
cargo run --release -p dex_engine --bin create_balance_manager -- --config config/bot.dex.example.json
```

After success, add to your bot JSON:

```json
"walletAddress": "0x...",
"balanceManagerId": "0x..."
```

Then fund the manager (deposit SUI / DBUSDC) via DeepBook SDK or a future deposit helper before trading.
