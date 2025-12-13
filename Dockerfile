# Multi-stage build for Trading System Runner
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml ./
COPY rust-toolchain.toml ./
# Cargo.lock is optional - will be generated if missing
COPY Cargo.lock* ./

# Copy workspace members
COPY libs/trading_core ./libs/trading_core
COPY src/exchange_engine ./src/exchange_engine
COPY src/trading_engine ./src/trading_engine
COPY src/market_data_engine ./src/market_data_engine
COPY src/indicator_engine ./src/indicator_engine
COPY src/asset_engine ./src/asset_engine
COPY src/live_config ./src/live_config
COPY src/runner ./src/runner

# Build dependencies first (caching layer)
RUN cargo build --release --package runner || true

# Copy source code (this invalidates cache if source changes)
COPY . .

# Build only the main runner binary (exclude test_runner)
# Remove test_runner binary definition from Cargo.toml since the file is excluded
RUN sed -i '/^\[\[bin\]\]$/,/^path = "src\/bin\/test_runner.rs"$/d' src/runner/Cargo.toml && \
    cargo build --release --package runner

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 appuser

# Set working directory
WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/runner /app/trading-system

# Copy sample_config and create config directory from it
# (config will be mounted as volume at runtime in docker-compose)
COPY sample_config ./sample_config
RUN mkdir -p ./config && cp -r ./sample_config/* ./config/

# Create logs directory
RUN mkdir -p /app/logs && chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose gRPC and health ports
EXPOSE 50051 8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the trading system
CMD ["/app/trading-system", "--mode", "levels"]

