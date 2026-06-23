# DeepBook setup utilities

Tools for one-off Sui / DeepBook setup and running the DEX levels bot on testnet.

## Prerequisites

- Rust toolchain (`cargo`)
- Testnet SUI in your wallet (gas) — [Sui testnet faucet](https://faucet.sui.io/)
- Signing key in `dex_engine.config.privateKey` (or set `privateKeyEnv` + export that env var as fallback)

Optional for dex-only mode:

- MongoDB is **not** required when `venue.mode: dex` and `mongo.enabled: false`

## Config secrets

Confidential fields use `Secret<SmolStr>` / `Secret<String>` in code — they redact in logs (`Debug` shows `***REDACTED***`) but serialize normally in JSON for local config files.

**Preferred:** put the signing key inline (same file the DEX runner uses):

```json
"privateKey": "suiprivkey1...",
"walletAddress": "0x...",
"balanceManagerId": "0x..."
```

Scripts default to `config/dex/bot.json` (then `config/bot.dex.local.json`, then `config/bot.dex.example.json`). No `export SUI_PRIVATE_KEY` needed when `privateKey` is set.

Use `privateKeyEnv` + export only if you intentionally keep the key out of JSON (see `config/bot.dex.local.json`).

## 1. Create BalanceManager (once)

Creates, registers, and shares a DeepBook V3 `BalanceManager`. Prints `balanceManagerId` for your bot config.

```bash
./scripts/deepbook/create-balance-manager.sh
# or with a local config:
./scripts/deepbook/create-balance-manager.sh config/bot.dex.example.json
```

Paste `walletAddress` and `balanceManagerId` into `dex_engine.config`.

## 2. Deposit trading funds

Wallet balance ≠ BalanceManager balance. Deposit before placing orders:

- **Buy quotes** need **DBUSDC** in the BalanceManager
- **Sell quotes** need **SUI** in the BalanceManager

```bash
just dex-deposit --sui 2 --dbusdc 50
# or
./scripts/deepbook/deposit.sh --sui 2 --dbusdc 50
```

Direct cargo invocation:

```bash
cargo run --release -p deepbook_tools --bin deposit_to_balance_manager -- \
  --config config/bot.dex.example.json --sui 2 --dbusdc 50
```

## 3. Configure quotes

Copy and edit the example levels file (prices must respect pool tick/lot/min):

```bash
cp config/dex_levels.example.json config/dex_levels.json
# Edit bid/ask prices — example uses 3.50 / 4.00 on SUI_DBUSDC, qty 1.0 SUI
```

## 4. Run the bot

```bash
cargo run --release -p runner -- \
  --mode dex \
  --config config/bot.dex.example.json \
  --dex-levels-config config/dex_levels.example.json
```

### Expected startup logs

```
=== Running in DEX levels mode ===
Loaded N DEX quote(s) from config/dex_levels.example.json
DeepBook pool loaded (chain Sui): SUI_DBUSDC ...
gRPC server started on [::]:50051 (DEX monitoring)
DEX levels engine started (N quote(s))
```

### Expected when placing

```
DEX quote placed quote_id=bid_1 signature=0x... status=Pending
```

Verify on testnet indexer:

```bash
curl -s "https://deepbook-indexer.testnet.mystenlabs.com/orders/SUI_DBUSDC/<balanceManagerId>?status=Placed"
```

### Shutdown

Ctrl-C submits **cancel-all** for each configured pool before exit.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `missing signing key` | Add `privateKey` under `dex_engine.config` in your bot JSON, or export the env named by `privateKeyEnv` |
| `walletAddress does not match signing key` | Update config wallet or use the key for that address |
| `balanceManagerId required` | Run create-balance-manager first |
| `insufficient balance for deposit` | Fund wallet via faucet / bridge testnet DBUSDC |
| `invalid place command` / tick/lot error | Align price/qty in `dex_levels.json` with pool constraints |
| MongoDB connection failed | Set `mongo.enabled: false` for dex-only, or start Mongo |

## Tools (`deepbook_tools` crate)

| Command | Purpose |
|---------|---------|
| `deepbook_tools::create_balance_manager` | One-off BalanceManager setup |
| `deepbook_tools::deposit_to_balance_manager` | Move SUI/DBUSDC wallet → BalanceManager |
| `runner --mode dex` | Fixed-price quoting + repost-on-fill |
