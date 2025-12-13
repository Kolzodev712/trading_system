#!/bin/bash
# Encrypt configuration file
#
# Usage: ./scripts/encrypt_config.sh [input_file] [output_file] [password]
#
# Examples:
#   ./scripts/encrypt_config.sh config/bot.json config/bot.json.enc
#   ./scripts/encrypt_config.sh config/bot.json config/bot.json.enc "my_password"

set -e

INPUT="${1:-config/bot.json}"
OUTPUT="${2:-config/bot.json.enc}"

# Get password (from arg, env var, or prompt)
if [ -n "$3" ]; then
    PASSWORD="$3"
elif [ -n "$CONFIG_PASSWORD" ]; then
    PASSWORD="$CONFIG_PASSWORD"
else
    echo -n "Enter encryption password: "
    read -s PASSWORD
    echo
fi

if [ -z "$PASSWORD" ]; then
    echo "Error: Password cannot be empty"
    exit 1
fi

# Build the binary if needed
if [ ! -f "target/release/encrypt_config" ] && [ ! -f "target/debug/encrypt_config" ]; then
    echo "Building encrypt_config binary..."
    cargo build --release --package trading_core --bin encrypt_config 2>/dev/null || cargo build --package trading_core --bin encrypt_config
fi

# Run encryption
if [ -f "target/release/encrypt_config" ]; then
    ./target/release/encrypt_config "$INPUT" "$OUTPUT" "$PASSWORD"
else
    ./target/debug/encrypt_config "$INPUT" "$OUTPUT" "$PASSWORD"
fi

echo "Remember to:"
echo "  1. Delete or secure the plain file: $INPUT"
echo "  2. Set CONFIG_PASSWORD env var or enter password when starting the system"

