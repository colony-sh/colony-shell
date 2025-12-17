#!/usr/bin/env bash
# Convenient wrapper for running Colony BATS tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Build colony if needed
if [[ ! -f "$PROJECT_ROOT/target/debug/colony" ]]; then
    echo "Building colony..."
    cd "$PROJECT_ROOT" && cargo build --bin colony
fi

# Ensure BATS is available
if [[ ! -f "/tmp/bats-core/bin/bats" ]]; then
    echo "Installing BATS to /tmp/bats-core..."
    git clone --depth 1 https://github.com/bats-core/bats-core.git /tmp/bats-core
fi

# Run tests
echo "Running Colony BATS tests..."
echo "Colony binary: $PROJECT_ROOT/target/debug/colony"
echo ""

COLONY_BIN="$PROJECT_ROOT/target/debug/colony" \
    /tmp/bats-core/bin/bats "$@" "$SCRIPT_DIR/bash-compat"/*.bats

echo ""
echo "To run specific tests:"
echo "  $0 bash-compat/01_builtins.bats"
echo ""
echo "For verbose output:"
echo "  $0 --print-output-on-failure bash-compat/01_builtins.bats"
