#!/bin/bash
# Kill any process using port 50051 (gRPC server port)

PORT=50051

echo "Finding processes using port $PORT..."

# Try different methods to find the process
PID=$(lsof -ti :$PORT 2>/dev/null)

if [ -z "$PID" ]; then
    # Try with ss
    PID=$(ss -tlnp | grep :$PORT | grep -oP 'pid=\K[0-9]+' | head -1)
fi

if [ -z "$PID" ]; then
    # Try with fuser
    PID=$(fuser $PORT/tcp 2>/dev/null | awk '{print $1}')
fi

if [ -n "$PID" ]; then
    echo "Found process $PID using port $PORT"
    echo "Killing process $PID..."
    kill -9 $PID 2>/dev/null
    sleep 1
    echo "Port $PORT should now be free"
else
    echo "No process found using port $PORT"
    echo "Trying to find any 'runner' processes..."
    pkill -f "target/debug/runner" 2>/dev/null
    pkill -f "target/release/runner" 2>/dev/null
    echo "Done"
fi

