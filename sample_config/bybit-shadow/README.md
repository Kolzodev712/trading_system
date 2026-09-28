# Bybit Shadow — Candidate #2

Launch with the composition binary (the default `runner` binary rejects this strategy):

```text
cargo run -p btc_price_ma_app -- --mode strategies \
  --config sample_config/bybit-shadow/bot.json \
  --strategies-config sample_config/bybit-shadow/strategies.json
```

`initially_enabled` keeps the model evaluating in Shadow after snapshot restore. Offline research remains:

```text
cargo run -p research_cli -- btc-price-ma run ...
cargo run -p research_cli -- donchian-daily evaluate ...
```
