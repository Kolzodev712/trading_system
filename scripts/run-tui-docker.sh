#!/bin/bash
# Run TUI CLI in Docker with proper networking
# Usage: ./scripts/run-tui-docker.sh [endpoint]
# Default endpoint: http://trading-system:50051 (for Docker network)

set -e

# Get the network name from docker-compose
NETWORK_NAME="trading_system_trading-network"

# Check if network exists
if ! docker network inspect "$NETWORK_NAME" >/dev/null 2>&1; then
    echo "Error: Network '$NETWORK_NAME' not found."
    echo "Make sure docker-compose services are running: docker-compose up -d"
    exit 1
fi

# Get endpoint from argument or use default
ENDPOINT="${1:-http://trading-system:50051}"

# Check if CLI image exists
if ! docker images | grep -q "trading-system-cli"; then
    echo "Building CLI image..."
    docker build -t trading-system-cli -f Dockerfile.cli .
fi

# Run TUI CLI in Docker
echo "Starting TUI CLI..."
echo "Connecting to: $ENDPOINT"
echo "Press Ctrl+C to exit"
echo ""

docker run -it --rm \
    --network "$NETWORK_NAME" \
    trading-system-cli \
    "$ENDPOINT"

