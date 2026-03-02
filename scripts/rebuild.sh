#!/bin/bash

# Smart rebuild script - rebuilds trading-system and CLI only if changes detected
# Usage: ./rebuild.sh [--no-cache] [--force] [--cli-only] [--system-only] [--observability]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

NO_CACHE=false
FORCE=false
CLI_ONLY=false
SYSTEM_ONLY=false
OBSERVABILITY=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --no-cache)
            NO_CACHE=true
            shift
            ;;
        --force)
            FORCE=true
            shift
            ;;
        --cli-only)
            CLI_ONLY=true
            shift
            ;;
        --system-only)
            SYSTEM_ONLY=true
            shift
            ;;
        --observability)
            OBSERVABILITY=true
            shift
            ;;
        *)
            echo "Unknown option: $arg"
            echo "Usage: $0 [--no-cache] [--force] [--cli-only] [--system-only] [--observability]"
            exit 1
            ;;
    esac
done

# Compose file(s) for docker-compose commands
COMPOSE_FILES="-f docker-compose.yml"
if [[ "$OBSERVABILITY" == "true" ]]; then
    COMPOSE_FILES="-f docker-compose.yml -f docker-compose.observability.yml"
fi

# Function to check if files changed for a service
check_changes() {
    local service=$1
    local dockerfile="Dockerfile"
    
    if [[ "$service" == "cli" ]]; then
        dockerfile="Dockerfile.cli"
    fi
    
    # If force rebuild, always rebuild
    if [[ "$FORCE" == "true" ]]; then
        return 0  # Changes detected
    fi
    
    # Check if Dockerfile or source files changed
    local image_name="trading-system"
    if [[ "$service" == "cli" ]]; then
        image_name="trading-system-cli"
    fi
    
    # Check if image exists
    if ! docker images --format "{{.Repository}}" | grep -q "^${image_name}$"; then
        return 0  # Image doesn't exist, rebuild
    fi
    
    # Get image creation time as Unix timestamp
    local image_created=$(docker inspect -f '{{.Created}}' "${image_name}:latest" 2>/dev/null)
    if [[ -z "$image_created" ]]; then
        return 0  # Can't get image time, rebuild to be safe
    fi
    
    # Convert image creation time to Unix timestamp (works on both Linux and macOS)
    local image_timestamp
    if command -v date >/dev/null 2>&1; then
        # Try Linux format first
        image_timestamp=$(date -d "$image_created" +%s 2>/dev/null || \
                         date -j -f "%Y-%m-%dT%H:%M:%S" "${image_created%.*}" +%s 2>/dev/null || \
                         echo "0")
    else
        image_timestamp="0"
    fi
    
    # Check if Dockerfile is newer
    if [[ -f "$dockerfile" ]]; then
        local dockerfile_time=$(stat -c %Y "$dockerfile" 2>/dev/null || stat -f %m "$dockerfile" 2>/dev/null || echo "0")
        if [[ "$dockerfile_time" -gt "$image_timestamp" ]]; then
            return 0  # Dockerfile changed
        fi
    fi
    
    # Check Cargo files
    if [[ -f "Cargo.toml" ]]; then
        local cargo_time=$(stat -c %Y "Cargo.toml" 2>/dev/null || stat -f %m "Cargo.toml" 2>/dev/null || echo "0")
        if [[ "$cargo_time" -gt "$image_timestamp" ]]; then
            return 0  # Cargo.toml changed
        fi
    fi
    
    if [[ -f "Cargo.lock" ]]; then
        local lock_time=$(stat -c %Y "Cargo.lock" 2>/dev/null || stat -f %m "Cargo.lock" 2>/dev/null || echo "0")
        if [[ "$lock_time" -gt "$image_timestamp" ]]; then
            return 0  # Cargo.lock changed
        fi
    fi
    
    # Check relevant source directories
    local dirs_to_check=()
    if [[ "$service" == "cli" ]]; then
        dirs_to_check=("cli/src" "libs/trading_core")
    else
        dirs_to_check=("src/runner" "src/trading_engine" "src/exchange_engine" "src/market_data_engine" "libs/trading_core")
    fi
    
    for dir in "${dirs_to_check[@]}"; do
        if [[ ! -d "$dir" ]]; then
            continue
        fi
        
        # Find files newer than image
        if find "$dir" -type f -newermt "$image_created" 2>/dev/null | head -1 | grep -q .; then
            return 0  # Source files changed
        fi
        
        # Fallback: check modification time of directory
        local dir_time=$(stat -c %Y "$dir" 2>/dev/null || stat -f %m "$dir" 2>/dev/null || echo "0")
        if [[ "$dir_time" -gt "$image_timestamp" ]]; then
            return 0  # Directory modified
        fi
    done
    
    return 1  # No changes detected
}

# Function to rebuild a service
rebuild_service() {
    local service=$1
    local dockerfile="Dockerfile"
    local build_args=""
    
    if [[ "$service" == "cli" ]]; then
        dockerfile="Dockerfile.cli"
        echo "🔨 Building CLI image..."
        if [[ "$NO_CACHE" == "true" ]]; then
            docker build --no-cache -t trading-system-cli -f "$dockerfile" .
        else
            docker build -t trading-system-cli -f "$dockerfile" .
        fi
    else
        echo "🔨 Building trading-system image..."
        if [[ "$NO_CACHE" == "true" ]]; then
            docker-compose $COMPOSE_FILES build --no-cache trading-system
        else
            docker-compose $COMPOSE_FILES build trading-system
        fi
    fi
}

echo "🔍 Checking for changes..."

# Determine what to rebuild
REBUILD_SYSTEM=false
REBUILD_CLI=false

if [[ "$SYSTEM_ONLY" == "true" ]]; then
    REBUILD_SYSTEM=true
elif [[ "$CLI_ONLY" == "true" ]]; then
    REBUILD_CLI=true
else
    # Check both services
    if check_changes "system"; then
        REBUILD_SYSTEM=true
        echo "✅ Changes detected in trading-system, will rebuild"
    else
        echo "⏭️  No changes in trading-system, skipping rebuild"
    fi
    
    if check_changes "cli"; then
        REBUILD_CLI=true
        echo "✅ Changes detected in CLI, will rebuild"
    else
        echo "⏭️  No changes in CLI, skipping rebuild"
    fi
fi

# If nothing to rebuild and not forced, exit
if [[ "$REBUILD_SYSTEM" == "false" ]] && [[ "$REBUILD_CLI" == "false" ]] && [[ "$FORCE" == "false" ]]; then
    echo ""
    echo "✅ No changes detected, nothing to rebuild!"
    echo "   Use --force to force rebuild anyway"
    exit 0
fi

# Stop and remove containers if rebuilding system (fixes docker-compose ContainerConfig errors)
if [[ "$REBUILD_SYSTEM" == "true" ]]; then
    echo ""
    echo "🛑 Stopping and removing containers..."
    # Use docker-compose down to properly clean up all containers and avoid ContainerConfig errors
    docker-compose $COMPOSE_FILES down 2>/dev/null || true
    # Also remove containers directly as fallback (handles stale containers)
    docker rm -f trading-system trading-system-mongodb trading-jaeger trading-prometheus trading-grafana 2>/dev/null || true
fi

# Rebuild services
if [[ "$REBUILD_SYSTEM" == "true" ]]; then
    rebuild_service "system"
fi

if [[ "$REBUILD_CLI" == "true" ]]; then
    rebuild_service "cli"
fi

# Start containers if we rebuilt system, OR if user asked for observability (bring up full stack)
if [[ "$REBUILD_SYSTEM" == "true" ]]; then
    echo ""
    echo "🚀 Starting containers..."
    # Use up -d to start all services (MongoDB will start first due to depends_on)
    # Don't use --force-recreate here since we already did docker-compose down
    docker-compose $COMPOSE_FILES up -d
    
    echo "📊 Waiting for services to be ready..."
    sleep 5
    
    echo "📋 Container status:"
    docker-compose $COMPOSE_FILES ps
elif [[ "$OBSERVABILITY" == "true" ]]; then
    echo ""
    echo "🚀 Starting full stack (with observability)..."
    docker-compose $COMPOSE_FILES up -d
    echo "📋 Container status:"
    docker-compose $COMPOSE_FILES ps
fi

echo ""
echo "✅ Rebuild complete!"
echo ""
if [[ "$REBUILD_SYSTEM" == "true" ]] || [[ "$OBSERVABILITY" == "true" ]]; then
    echo "To view logs:"
    echo "  docker-compose $COMPOSE_FILES logs -f trading-system"
    if [[ "$OBSERVABILITY" == "true" ]]; then
        echo ""
        echo "Observability: Jaeger http://localhost:16686 | Grafana http://localhost:3000 | Prometheus http://localhost:9090"
    fi
fi
if [[ "$REBUILD_CLI" == "true" ]]; then
    echo "CLI image rebuilt. To run TUI:"
    echo "  ./scripts/run-tui-docker.sh"
fi
