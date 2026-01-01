# Trading System - TODO & Improvement Roadmap

## Trading Functionality & API Utilization

### Futures & Perpetuals Support
- [ ] **Futures exchange integration**
  - Add futures/perps support to Binance client (USDT-M, COIN-M futures)
  - Support position modes (one-way, hedge mode)
  - Support margin types (isolated, cross margin)
  - Leverage management (set leverage, query current leverage)
  - Position management (open positions, position size, P&L)

- [ ] **Futures-specific order types**
  - Stop market orders
  - Stop limit orders
  - Trailing stop orders
  - Take profit orders
  - Reduce-only orders
  - Close position orders

- [ ] **Futures risk management**
  - Maintenance margin checks
  - Liquidation price tracking
  - Position size limits (with leverage consideration)
  - Margin call handling
  - Auto-deleveraging (ADL) awareness

- [ ] **Futures market data**
  - Funding rate streaming
  - Mark price updates
  - Open interest data
  - Basis (spot-futures spread)

### Advanced Order Types & Features
- [ ] **Order type expansion**
  - Iceberg orders (display quantity)
  - Time-in-force options (GTC, IOC, FOK, Post-Only)
  - Conditional orders (stop-loss, take-profit)
  - OCO (One-Cancels-Other) orders
  - TWAP (Time-Weighted Average Price) orders
  - VWAP (Volume-Weighted Average Price) orders

- [ ] **Order management enhancements**
  - Batch order placement (multiple orders in one request)
  - Order modification (amend price/quantity without cancel+place)
  - Partial fills handling
  - Order expiration handling
  - Order status polling (for exchanges without WebSocket order updates)

- [ ] **Exchange-specific optimizations**
  - Use exchange-specific features (e.g., Binance's test order endpoint)
  - Optimize for exchange rate limits (request weight optimization)
  - Use exchange-specific order types when available

### Market Data Enhancements
- [ ] **Advanced market data**
  - Depth snapshots (full orderbook depth)
  - Aggregated trades (for better volume analysis)
  - Kline/candlestick data streaming
  - 24hr ticker statistics
  - Exchange info (trading rules, filters, limits)

- [ ] **Market data utilization**
  - Use market data for dynamic order sizing
  - Use orderbook depth for slippage estimation
  - Use volume data for position sizing
  - Use ticker data for price validation

## System Stability & Robustness

### Error Handling & Recovery
- [ ] **Exchange API error handling**
  - Handle all exchange error codes (INSUFFICIENT_BALANCE, MIN_NOTIONAL, etc.)
  - Automatic retry with exponential backoff for transient errors
  - Circuit breaker for persistent exchange failures
  - Fallback strategies when exchange unavailable

- [ ] **WebSocket reliability**
  - Automatic reconnection with exponential backoff
  - Message sequence validation (detect gaps, request snapshots)
  - Heartbeat/ping-pong handling
  - Connection health monitoring

- [ ] **State reconciliation**
  - Periodic reconciliation with exchange (every 3-5 minutes)
  - On-demand reconciliation (after errors, on startup)
  - Stale order detection and cleanup
  - Position reconciliation (compare local vs exchange)

### Configuration & Validation
- [ ] **Startup validation**
  - Validate exchange credentials (test connection)
  - Validate all config files (JSON schema)
  - Validate level configurations (price/quantity ranges, valid indicators)
  - Validate risk limits (no conflicting limits)
  - Pre-flight checks (MongoDB connectivity, exchange API access)

- [ ] **Runtime validation**
  - Validate order parameters before placement (price precision, quantity min/max)
  - Validate risk limits before order placement
  - Validate market data before using (price sanity checks, volume checks)

### Observability
- [ ] **Metrics & monitoring**
  - Prometheus metrics (order latency, success rate, exchange API errors)
  - Health check endpoints (readiness, liveness)
  - Structured logging with correlation IDs
  - Alerting for critical failures (circuit breaker, exchange disconnection)

- [ ] **Performance tracking**
  - Order placement latency tracking
  - Risk check latency tracking
  - Channel queue depth monitoring
  - Memory usage monitoring

### Testing & Quality
- [ ] **Test coverage**
  - Unit tests for all order types
  - Integration tests for futures operations
  - Testnet integration tests (daily automated runs)
  - Chaos testing (simulate exchange failures, network issues)

- [ ] **Performance testing**
  - Load testing (1000+ orders/second)
  - Latency benchmarking
  - Memory leak detection

## Potential Tech Architecture (Dual-UI Strategy)

### Web UI Architecture
- [ ] **Web-based trader control panel**
  - REST API or gRPC gateway for web UI communication
  - WebSocket connection for real-time updates (orderbook, positions, P&L)
  - Authentication & authorization (API keys, session management)
  - Level/strategy configuration interface (forms, visual editors)
  - Portfolio dashboard (positions, balances, performance charts)
  - Historical data visualization (charts, reports)

- [ ] **Web UI backend service**
  - Separate service or integrated into runner
  - HTTP server (Axum/Actix) for REST endpoints
  - WebSocket server for real-time streaming
  - Data transformation layer (trading system format → web UI format)

### TUI Enhancements (Technical Control Panel)
- [ ] **Debugging & monitoring features**
  - Real-time log streaming (filterable, searchable)
  - System metrics dashboard (channel depths, latency, memory)
  - Component health status (exchange engine, market data, trading engine)
  - Performance profiling (order latency breakdown, risk check timing)

- [ ] **Emergency controls**
  - Kill switch (immediate stop all trading)
  - Circuit breaker controls (manual trip/reset)
  - Force cancel all orders
  - Emergency position close
  - Risk limit override (temporary, with audit log)

- [ ] **Quick actions**
  - Pause/resume trading per pair
  - Adjust risk limits on-the-fly
  - Force reconciliation
  - Manual order cancellation
  - System state inspection (dump state to file)

### API Architecture
- [ ] **REST API for web UI**
  - Level management (CRUD operations)
  - Strategy management
  - Risk limit management
  - System status & health
  - Historical data queries

- [ ] **WebSocket API for real-time updates**
  - Order updates stream
  - Position updates stream
  - Market data stream (orderbook, trades)
  - Risk metrics stream
  - System events stream (errors, alerts)

- [ ] **gRPC API (existing)**
  - Keep for TUI CLI
  - Add web UI support (via gRPC-web or gateway)

### Documentation Infrastructure
- [ ] **API documentation**
  - OpenAPI/Swagger spec for REST API
  - gRPC proto documentation
  - Code examples (Rust, Python, JavaScript)
  - Postman collection

- [ ] **Configuration documentation**
  - Configuration schema documentation
  - Example configurations (common strategies)
  - Configuration validation rules
  - Best practices guide

- [ ] **Technical documentation**
  - Architecture diagrams
  - Deployment guide
  - Troubleshooting guide
  - Performance tuning guide

## Commercial Requirements (Short List)

### Multi-Tenancy
- [ ] Multi-tenant architecture (isolated by bot_id)
- [ ] Resource limits per tenant
- [ ] Tenant usage tracking

### Licensing & Access
- [ ] License key validation
- [ ] Feature flags by license tier
- [ ] User authentication & RBAC
- [ ] API rate limiting per tenant

### Billing & Support
- [ ] Usage metrics export
- [ ] Billing integration (Stripe/Paddle)
- [ ] Customer dashboard (web UI)
- [ ] Support ticket integration

### Compliance
- [ ] Terms of service & SLA
- [ ] Data encryption (at rest, in transit)
- [ ] Audit logging
- [ ] GDPR compliance (data export/deletion)

---

## Priority Order

1. **Futures/perps integration** - Core trading functionality
2. **Advanced order types** - Full API utilization
3. **Error handling & recovery** - System stability
4. **State reconciliation** - Data integrity
5. **Observability** - Production readiness
6. **Multi-tenancy** - Commercial requirement
