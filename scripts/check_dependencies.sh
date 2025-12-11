#!/bin/bash
# Check if all dependencies are running and ports are free before starting the bot
# If dependencies are not running, attempts to start them automatically

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
CONFIG_FILE="config/bot.json"
MONGO_PORT=27017
GRPC_PORT=50051
HEALTH_PORT=8080

# Check if config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${RED}Error: Config file not found: $CONFIG_FILE${NC}"
    exit 1
fi

# Extract MongoDB connection string from config
MONGO_CONNECTION=$(grep -o '"connection_string": "[^"]*"' "$CONFIG_FILE" | cut -d'"' -f4)
MONGO_HOST=$(echo "$MONGO_CONNECTION" | sed -n 's|mongodb://\([^:]*\):.*|\1|p')
MONGO_PORT_FROM_CONFIG=$(echo "$MONGO_CONNECTION" | sed -n 's|mongodb://[^:]*:\([0-9]*\).*|\1|p')

# Use port from config if available, otherwise default
if [ -n "$MONGO_PORT_FROM_CONFIG" ]; then
    MONGO_PORT=$MONGO_PORT_FROM_CONFIG
fi

# Extract health server port from config
HEALTH_PORT_FROM_CONFIG=$(grep -o '"port": [0-9]*' "$CONFIG_FILE" | head -1 | grep -o '[0-9]*')
if [ -n "$HEALTH_PORT_FROM_CONFIG" ]; then
    HEALTH_PORT=$HEALTH_PORT_FROM_CONFIG
fi

# Check if Redis is enabled
REDIS_ENABLED=$(grep -o '"enabled": [a-z]*' "$CONFIG_FILE" | grep -A1 redis | tail -1 | grep -o '[a-z]*')
REDIS_PORT=6379

echo "=========================================="
echo "Trading System Dependency Check"
echo "=========================================="
echo ""

ERRORS=0
WARNINGS=0

# Function to check if a port is free
check_port_free() {
    local port=$1
    local service=$2
    
    if command -v lsof >/dev/null 2>&1; then
        if lsof -ti :$port >/dev/null 2>&1; then
            echo -e "${RED}✗${NC} Port $port ($service) is in use"
            local pid=$(lsof -ti :$port)
            local process=$(ps -p $pid -o comm= 2>/dev/null || echo "unknown")
            echo "  → Process: $process (PID: $pid)"
            return 1
        fi
    elif command -v ss >/dev/null 2>&1; then
        if ss -tlnp | grep -q ":$port "; then
            echo -e "${RED}✗${NC} Port $port ($service) is in use"
            return 1
        fi
    elif command -v netstat >/dev/null 2>&1; then
        if netstat -tuln 2>/dev/null | grep -q ":$port "; then
            echo -e "${RED}✗${NC} Port $port ($service) is in use"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠${NC} Cannot check port $port (no port checking tool available)"
        return 0
    fi
    
    echo -e "${GREEN}✓${NC} Port $port ($service) is free"
    return 0
}

# Function to start a service in a new terminal window
start_in_terminal() {
    local service_name=$1
    local start_command=$2
    
    # Detect terminal emulator
    local terminal=""
    if command -v gnome-terminal >/dev/null 2>&1; then
        terminal="gnome-terminal"
    elif command -v xterm >/dev/null 2>&1; then
        terminal="xterm"
    elif command -v konsole >/dev/null 2>&1; then
        terminal="konsole"
    elif command -v x-terminal-emulator >/dev/null 2>&1; then
        terminal="x-terminal-emulator"
    elif [ -n "$TERMINAL" ]; then
        terminal="$TERMINAL"
    fi
    
    if [ -z "$terminal" ]; then
        echo -e "${YELLOW}  → No terminal emulator found. Please start $service_name manually.${NC}"
        return 1
    fi
    
    echo -e "${YELLOW}  → Starting $service_name in new terminal window...${NC}"
    
    case "$terminal" in
        gnome-terminal)
            gnome-terminal --title="$service_name" -- bash -c "$start_command; echo 'Press Enter to close...'; read" &
            ;;
        xterm)
            xterm -T "$service_name" -e bash -c "$start_command; echo 'Press Enter to close...'; read" &
            ;;
        konsole)
            konsole --title "$service_name" -e bash -c "$start_command; echo 'Press Enter to close...'; read" &
            ;;
        *)
            $terminal -e bash -c "$start_command; echo 'Press Enter to close...'; read" &
            ;;
    esac
    
    # Wait a bit for the service to start
    sleep 2
    return 0
}

# Function to check MongoDB connection
check_mongodb() {
    local host=${MONGO_HOST:-localhost}
    local port=$MONGO_PORT
    
    echo "Checking MongoDB connection..."
    echo "  Host: $host"
    echo "  Port: $port"
    
    # Try to connect using mongosh or mongo client
    if command -v mongosh >/dev/null 2>&1; then
        if mongosh --host "$host" --port "$port" --eval "db.adminCommand('ping')" --quiet >/dev/null 2>&1; then
            echo -e "${GREEN}✓${NC} MongoDB is running and accessible"
            return 0
        fi
    elif command -v mongo >/dev/null 2>&1; then
        if mongo --host "$host" --port "$port" --eval "db.adminCommand('ping')" --quiet >/dev/null 2>&1; then
            echo -e "${GREEN}✓${NC} MongoDB is running and accessible"
            return 0
        fi
    fi
    
    # Fallback: try TCP connection
    if command -v nc >/dev/null 2>&1; then
        if nc -z "$host" "$port" 2>/dev/null; then
            echo -e "${GREEN}✓${NC} MongoDB port is open (connection test passed)"
            return 0
        fi
    elif command -v timeout >/dev/null 2>&1 && command -v bash >/dev/null 2>&1; then
        # Try with bash TCP connection
        if timeout 1 bash -c "echo > /dev/tcp/$host/$port" 2>/dev/null; then
            echo -e "${GREEN}✓${NC} MongoDB port is open (connection test passed)"
            return 0
        fi
    fi
    
    echo -e "${RED}✗${NC} MongoDB is not accessible"
    
    # Try to start MongoDB
    echo "  → Attempting to start MongoDB..."
    
    # Try systemd first
    if systemctl is-active --quiet mongod 2>/dev/null || systemctl is-active --quiet mongodb 2>/dev/null; then
        echo -e "${GREEN}  → MongoDB service is active but not responding. Checking logs...${NC}"
    elif systemctl list-unit-files | grep -q mongod.service 2>/dev/null || systemctl list-unit-files | grep -q mongodb.service 2>/dev/null; then
        echo "  → Starting MongoDB via systemd..."
        if sudo systemctl start mongod 2>/dev/null || sudo systemctl start mongodb 2>/dev/null; then
            sleep 2
            if check_mongodb_connection "$host" "$port"; then
                echo -e "${GREEN}  → MongoDB started successfully via systemd${NC}"
                return 0
            fi
        fi
    fi
    
    # Try Docker
    if command -v docker >/dev/null 2>&1; then
        if docker ps -a --format '{{.Names}}' | grep -q mongo 2>/dev/null; then
            local container=$(docker ps -a --format '{{.Names}}' | grep mongo | head -1)
            echo "  → Starting MongoDB Docker container: $container"
            if docker start "$container" >/dev/null 2>&1; then
                sleep 3
                if check_mongodb_connection "$host" "$port"; then
                    echo -e "${GREEN}  → MongoDB started successfully via Docker${NC}"
                    return 0
                fi
            fi
        fi
    fi
    
    # Try to start mongod directly in a terminal
    if command -v mongod >/dev/null 2>&1; then
        local mongo_data_dir="${HOME}/data/db"
        if [ ! -d "$mongo_data_dir" ]; then
            mongo_data_dir="/var/lib/mongodb"
        fi
        if [ ! -d "$mongo_data_dir" ]; then
            mongo_data_dir="/data/db"
        fi
        
        local start_cmd="mongod --dbpath $mongo_data_dir"
        if start_in_terminal "MongoDB" "$start_cmd"; then
            sleep 3
            if check_mongodb_connection "$host" "$port"; then
                echo -e "${GREEN}  → MongoDB started successfully${NC}"
                return 0
            fi
        fi
    fi
    
    echo -e "${RED}  → Could not start MongoDB automatically${NC}"
    echo "  → Please start MongoDB manually:"
    echo "     - sudo systemctl start mongod"
    echo "     - docker start <mongodb-container>"
    echo "     - mongod --dbpath /data/db"
    return 1
}

# Helper function to check MongoDB connection (used after starting)
check_mongodb_connection() {
    local host=$1
    local port=$2
    
    if command -v mongosh >/dev/null 2>&1; then
        mongosh --host "$host" --port "$port" --eval "db.adminCommand('ping')" --quiet >/dev/null 2>&1
    elif command -v mongo >/dev/null 2>&1; then
        mongo --host "$host" --port "$port" --eval "db.adminCommand('ping')" --quiet >/dev/null 2>&1
    elif command -v nc >/dev/null 2>&1; then
        nc -z "$host" "$port" 2>/dev/null
    else
        timeout 1 bash -c "echo > /dev/tcp/$host/$port" 2>/dev/null
    fi
}

# Function to check Redis (if enabled)
check_redis() {
    if [ "$REDIS_ENABLED" != "true" ]; then
        echo -e "${GREEN}✓${NC} Redis is disabled (skipping check)"
        return 0
    fi
    
    echo "Checking Redis connection..."
    
    if command -v redis-cli >/dev/null 2>&1; then
        if redis-cli -p $REDIS_PORT ping >/dev/null 2>&1; then
            echo -e "${GREEN}✓${NC} Redis is running and accessible"
            return 0
        fi
    fi
    
    # Fallback: try TCP connection
    if command -v nc >/dev/null 2>&1; then
        if nc -z localhost $REDIS_PORT 2>/dev/null; then
            echo -e "${GREEN}✓${NC} Redis port is open (connection test passed)"
            return 0
        fi
    fi
    
    echo -e "${RED}✗${NC} Redis is not accessible (but enabled in config)"
    
    # Try to start Redis
    echo "  → Attempting to start Redis..."
    
    # Try systemd first
    if systemctl is-active --quiet redis 2>/dev/null || systemctl is-active --quiet redis-server 2>/dev/null; then
        echo -e "${GREEN}  → Redis service is active but not responding. Checking logs...${NC}"
    elif systemctl list-unit-files | grep -q redis.service 2>/dev/null || systemctl list-unit-files | grep -q redis-server.service 2>/dev/null; then
        echo "  → Starting Redis via systemd..."
        if sudo systemctl start redis 2>/dev/null || sudo systemctl start redis-server 2>/dev/null; then
            sleep 2
            if check_redis_connection; then
                echo -e "${GREEN}  → Redis started successfully via systemd${NC}"
                return 0
            fi
        fi
    fi
    
    # Try Docker
    if command -v docker >/dev/null 2>&1; then
        if docker ps -a --format '{{.Names}}' | grep -q redis 2>/dev/null; then
            local container=$(docker ps -a --format '{{.Names}}' | grep redis | head -1)
            echo "  → Starting Redis Docker container: $container"
            if docker start "$container" >/dev/null 2>&1; then
                sleep 2
                if check_redis_connection; then
                    echo -e "${GREEN}  → Redis started successfully via Docker${NC}"
                    return 0
                fi
            fi
        fi
    fi
    
    # Try to start redis-server directly in a terminal
    if command -v redis-server >/dev/null 2>&1; then
        local start_cmd="redis-server --port $REDIS_PORT"
        if start_in_terminal "Redis" "$start_cmd"; then
            sleep 2
            if check_redis_connection; then
                echo -e "${GREEN}  → Redis started successfully${NC}"
                return 0
            fi
        fi
    fi
    
    echo -e "${RED}  → Could not start Redis automatically${NC}"
    echo "  → Please start Redis manually:"
    echo "     - sudo systemctl start redis"
    echo "     - docker start <redis-container>"
    echo "     - redis-server"
    return 1
}

# Helper function to check Redis connection (used after starting)
check_redis_connection() {
    if command -v redis-cli >/dev/null 2>&1; then
        redis-cli -p $REDIS_PORT ping >/dev/null 2>&1
    elif command -v nc >/dev/null 2>&1; then
        nc -z localhost $REDIS_PORT 2>/dev/null
    else
        timeout 1 bash -c "echo > /dev/tcp/localhost/$REDIS_PORT" 2>/dev/null
    fi
}

# Check MongoDB
echo "1. MongoDB Dependency"
if ! check_mongodb; then
    ((ERRORS++))
fi
echo ""

# Check Redis (if enabled)
echo "2. Redis Dependency"
if ! check_redis; then
    ((ERRORS++))
fi
echo ""

# Check ports
echo "3. Port Availability"
echo ""

if ! check_port_free $GRPC_PORT "gRPC Server"; then
    ((ERRORS++))
fi

if ! check_port_free $HEALTH_PORT "Health Server"; then
    ((ERRORS++))
fi

echo ""

# Summary
echo "=========================================="
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}✓ All checks passed!${NC}"
    echo "You can safely start the bot."
    exit 0
else
    echo -e "${RED}✗ Found $ERRORS error(s)${NC}"
    echo ""
    echo "Please fix the issues above before starting the bot."
    echo ""
    echo "Common solutions:"
    echo "  • MongoDB: sudo systemctl start mongod"
    echo "  • Free ports: ./scripts/kill_grpc_port.sh"
    echo "  • Check processes: lsof -i :$GRPC_PORT or lsof -i :$HEALTH_PORT"
    exit 1
fi

