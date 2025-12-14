#!/bin/bash

# Rebuild and restart trading system Docker containers
# Usage: ./rebuild.sh [--no-cache]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🛑 Stopping containers..."
docker-compose down

if [[ "$1" == "--no-cache" ]]; then
    echo "🔨 Rebuilding Docker images (no cache)..."
    docker-compose build --no-cache trading-system
elif [[ "$1" == "--fast" ]]; then
    echo "🔨 Fast rebuild (using cache, only rebuild if needed)..."
    docker-compose build trading-system
else
    echo "🔨 Rebuilding Docker images (using cache)..."
    docker-compose build trading-system
fi

echo "🚀 Starting containers..."
docker-compose up -d

echo "📊 Waiting for services to be ready..."
sleep 3

echo "📋 Container status:"
docker-compose ps

echo ""
echo "✅ Rebuild complete!"
echo ""
echo "To view logs:"
echo "  docker-compose logs -f trading-system"
echo ""
echo "To view logs for a specific service:"
echo "  docker-compose logs -f <service-name>"

