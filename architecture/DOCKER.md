# Docker Guide

This guide explains how to run the Trading System using Docker containers.

## Prerequisites

### Docker Installation

Ensure Docker and Docker Compose are installed:

```bash
# Check Docker version
docker --version

# Check Docker Compose version
docker-compose --version
```

### Docker Permissions (Linux)

If you get a permission error like `PermissionError: [Errno 13] Permission denied`, you need to add your user to the `docker` group:

```bash
# Add your user to the docker group
sudo usermod -aG docker $USER

# Apply the changes (log out and back in, or use newgrp)
newgrp docker

# Verify you're in the docker group
groups
```

**Important**: After adding yourself to the docker group, you may need to:
- Log out and log back in, OR
- Run `newgrp docker` in your current terminal session

### Start Docker Daemon

Ensure Docker daemon is running:

```bash
# Check Docker daemon status
sudo systemctl status docker

# Start Docker daemon if not running
sudo systemctl start docker

# Enable Docker to start on boot
sudo systemctl enable docker
```

## Quick Start

### 1. Start the Trading System (with MongoDB)

```bash
# Start all services (MongoDB + Trading System)
docker-compose up -d

# View logs
docker-compose logs -f trading-system

# Check status
docker-compose ps
```

### 2. Update Configuration for Docker

**Important**: Before starting, update `config/bot.json` to use the Docker MongoDB service name:

```json
{
  "mongo": {
    "connection_string": "mongodb://mongodb:27017",
    "database": "trading_system",
    "bot_id": "main_bot",
    "enabled": true
  }
}
```

**Note**: If you're running MongoDB on the host (not in Docker), use `mongodb://host.docker.internal:27017` instead.

### 3. Run the TUI CLI

The CLI is interactive and should be run manually. You have two options:

#### Option A: Using Docker Run (Recommended)

```bash
# Build the CLI image first
docker build -t trading-system-cli -f Dockerfile.cli .

# Run the CLI connected to the trading system
docker run -it --rm \
  --network trading_system_trading-network \
  trading-system-cli \
  http://trading-system:50051
```

#### Option B: Using Docker Compose

```bash
# Build the CLI image
docker-compose build trading-cli

# Run the CLI interactively
docker-compose run --rm trading-cli http://trading-system:50051
```

## Detailed Instructions

### Prerequisites

1. **Docker** 20.10+ and **Docker Compose** 2.0+
2. **Configuration files** in `config/` directory
3. **API keys** configured in `config/bot.json`
4. **Docker permissions** (see above)

### Step-by-Step Setup

#### 1. Fix Docker Permissions (if needed)

If you encounter permission errors:

```bash
# Add user to docker group
sudo usermod -aG docker $USER

# Apply changes (choose one):
# Option 1: Log out and log back in
# Option 2: Use newgrp in current session
newgrp docker

# Verify
docker ps  # Should work without sudo
```

#### 2. Configure MongoDB Connection

Edit `config/bot.json` and update the MongoDB connection string:

**For Docker Compose** (MongoDB in container):
```json
{
  "mongo": {
    "connection_string": "mongodb://mongodb:27017",
    "database": "trading_system",
    "enabled": true
  }
}
```

**For Host MongoDB** (MongoDB on your machine):
```json
{
  "mongo": {
    "connection_string": "mongodb://host.docker.internal:27017",
    "database": "trading_system",
    "enabled": true
  }
}
```

#### 3. Configure API Keys

Edit `config/bot.json` and add your Binance API credentials:

```json
{
  "exchange": {
    "exchange": "Binance",
    "binance": {
      "rest": {
        "apiKey": "your_api_key",
        "secretKey": "your_secret_key"
      },
      "ws": {
        "apiKey": "your_api_key",
        "secretKey": "your_secret_key"
      }
    }
  }
}
```

#### 4. Start the Trading System

```bash
# Build and start all services
docker-compose up -d

# Check that services are running
docker-compose ps

# View logs
docker-compose logs -f trading-system
```

#### 5. Verify the System is Running

```bash
# Check health endpoint
curl http://localhost:8080/health

# Check gRPC server (should see connection attempt)
curl http://localhost:50051
```

#### 6. Run the TUI CLI

```bash
# Build CLI image (first time only)
docker build -t trading-system-cli -f Dockerfile.cli .

# Run CLI interactively
docker run -it --rm \
  --network trading_system_trading-network \
  trading-system-cli \
  http://trading-system:50051
```

## Docker Compose Services

### Services Overview

1. **mongodb**: MongoDB database for configuration persistence
   - Port: `27017:27017`
   - Volume: `mongodb_data` (persistent storage)

2. **trading-system**: Main trading system runner
   - Ports: `50051:50051` (gRPC), `8080:8080` (health)
   - Depends on: `mongodb`
   - Volumes: `./config:/app/config:ro` (read-only config)

3. **trading-cli**: TUI CLI (optional, run manually)
   - Interactive terminal interface
   - Connects to `trading-system` via gRPC

### Network

All services run on the `trading-network` bridge network, allowing them to communicate using service names:
- `mongodb` → `mongodb:27017`
- `trading-system` → `trading-system:50051`

## Common Commands

### Managing Services

```bash
# Start services
docker-compose up -d

# Stop services
docker-compose down

# Stop and remove volumes (clean slate)
docker-compose down -v

# View logs
docker-compose logs -f trading-system
docker-compose logs -f mongodb

# Restart a service
docker-compose restart trading-system
```

### Running CLI

```bash
# Quick run (assumes image is built)
docker run -it --rm \
  --network trading_system_trading-network \
  trading-system-cli \
  http://trading-system:50051

# Or using docker-compose
docker-compose run --rm trading-cli http://trading-system:50051
```

### Building Images

```bash
# Build runner image
docker build -t trading-system -f Dockerfile .

# Build CLI image
docker build -t trading-system-cli -f Dockerfile.cli .

# Build all images (via docker-compose)
docker-compose build
```

### Rebuilding After Code Changes

After making code changes, rebuild and restart the containers:

```bash
# Quick rebuild (uses cache)
./scripts/rebuild.sh

# Full rebuild (no cache, slower but ensures clean build)
./scripts/rebuild.sh --no-cache
```

The rebuild script will:
1. Stop all containers
2. Rebuild the trading-system image
3. Start all containers
4. Show container status

**Note**: The rebuild script is located in `scripts/rebuild.sh` and handles the full rebuild cycle automatically.

### Debugging

```bash
# Execute shell in running container
docker-compose exec trading-system /bin/bash

# View container logs
docker-compose logs trading-system

# Check container status
docker-compose ps

# Inspect network
docker network inspect trading_system_trading-network
```

## Running in Different Modes

### Levels Mode (Default)

```bash
# Start in levels mode (default)
docker-compose up -d

# Or explicitly
docker-compose up -d trading-system
```

### Strategies Mode

Edit `docker-compose.yml` and uncomment/modify the command:

```yaml
trading-system:
  # ...
  command: ["/app/trading-system", "--mode", "strategies", "--strategies-config", "/app/config/strategies.json"]
```

Or override when starting:

```bash
docker-compose run --rm trading-system \
  /app/trading-system --mode strategies --strategies-config /app/config/strategies.json
```

## Troubleshooting

### Docker Permission Issues

**Error**: `PermissionError: [Errno 13] Permission denied`

**Solutions**:
1. Add user to docker group:
   ```bash
   sudo usermod -aG docker $USER
   newgrp docker  # or log out and back in
   ```

2. Verify Docker daemon is running:
   ```bash
   sudo systemctl status docker
   sudo systemctl start docker  # if not running
   ```

3. Check Docker socket permissions:
   ```bash
   ls -la /var/run/docker.sock
   # Should show: srw-rw---- 1 root docker
   ```

4. As a temporary workaround (not recommended):
   ```bash
   sudo docker-compose up -d
   ```

### MongoDB Connection Issues

**Error**: `MongoDB connection failed`

**Solutions**:
1. Check MongoDB is running: `docker-compose ps mongodb`
2. Verify connection string in `config/bot.json`
3. Check network: `docker network inspect trading_system_trading-network`
4. Test connection: `docker-compose exec trading-system curl mongodb:27017`

### CLI Connection Issues

**Error**: `Failed to connect to gRPC server`

**Solutions**:
1. Ensure trading-system is running: `docker-compose ps`
2. Check gRPC port is exposed: `docker-compose port trading-system 50051`
3. Verify network: Use `--network trading_system_trading-network`
4. Check endpoint: Use `http://trading-system:50051` (not `localhost`)

### Port Conflicts

**Error**: `Bind for 0.0.0.0:50051 failed: port is already allocated`

**Solutions**:
1. Stop conflicting service: `docker-compose down`
2. Change ports in `docker-compose.yml`
3. Find and kill process: `lsof -i :50051`

### Configuration Issues

**Error**: `Failed to load config`

**Solutions**:
1. Verify config file exists: `ls -la config/bot.json`
2. Check JSON syntax: `cat config/bot.json | jq .`
3. Verify file permissions: `chmod 644 config/bot.json`
4. Check volume mount: `docker-compose exec trading-system ls -la /app/config`

## Environment Variables

You can override configuration using environment variables in `docker-compose.yml`:

```yaml
trading-system:
  environment:
    - RUST_LOG=debug
    - MONGO_CONNECTION_STRING=mongodb://mongodb:27017
```

## Volumes

### Persistent Data

- **MongoDB data**: Stored in `mongodb_data` volume (persists across restarts)
- **Logs**: Stored in `trading-logs` volume (optional)

### Config Files

Config files are mounted as read-only volumes:
- `./config:/app/config:ro`

**Note**: Changes to config files require container restart:
```bash
docker-compose restart trading-system
```

## Production Considerations

1. **Security**:
   - Use Docker secrets for API keys (not config files)
   - Restrict network access
   - Use read-only volumes where possible

2. **Monitoring**:
   - Set up log aggregation
   - Monitor container health
   - Use health checks

3. **Backup**:
   - Backup MongoDB volumes regularly
   - Backup configuration files
   - Document deployment process

4. **Scaling**:
   - Trading system is single-instance (not designed for horizontal scaling)
   - CLI can run multiple instances (read-only)

## Next Steps

- See [README.md](../README.md) for full system documentation
- See [Architecture Summary](../ARCHITECTURE.md) for architecture details
- Check logs: `docker-compose logs -f`
