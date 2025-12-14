#!/bin/bash

# Fast development rebuild - only rebuilds if code changed
# Usage: ./dev-rebuild.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🛑 Stopping containers..."
docker-compose stop trading-system

echo "🔨 Rebuilding Docker image (using cache - fast)..."
docker-compose build trading-system

echo "🚀 Starting containers..."
docker-compose up -d trading-system

echo "📊 Waiting for service to be ready..."
sleep 3

echo "📋 Container status:"
docker-compose ps trading-system

echo ""
echo "✅ Fast rebuild complete!"
echo ""
echo "To view logs:"
echo "  docker-compose logs -f trading-system"

