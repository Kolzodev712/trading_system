# OBSOLETE (2026-11-27): This Dockerfile does not match the current workspace
# (missing crates such as gateway/trading_proto/trading_networking, and previously
# suppressed a failed dependency build with `|| true`). Do not use for release or
# clean-checkout verification. Prefer Dockerfile.paper-bin or
# trading-system-showcase-demo. Kept only so historical compose references resolve.
FROM rust:1.98-slim AS builder

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

# Full tree required for a real build; the selective COPY above is incomplete.
COPY . .

# Fail the image build if the package cannot compile (do not suppress errors).
RUN cargo build --release --package runner

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

# Create config and logs directories
# Note: Config directory should be mounted as volume at runtime
RUN mkdir -p ./config
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

