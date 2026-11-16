#!/usr/bin/env bash
set -euo pipefail

# Build colony-shell with the TUI REPL (enabled by default)
#
# Usage:
#   ./scripts/build.sh           # debug build
#   ./scripts/build.sh release   # release build
#   ./scripts/build.sh install   # release build + install to ~/.cargo/bin

PROFILE="${1:-debug}"

case "$PROFILE" in
    debug)
        echo "Building colony (debug)..."
        cargo build --bin colony --bin colony-events
        echo "Binaries: target/debug/colony, target/debug/colony-events"
        ;;
    release)
        echo "Building colony (release)..."
        cargo build --bin colony --bin colony-events --release
        echo "Binaries: target/release/colony, target/release/colony-events"
        ;;
    install)
        echo "Installing colony..."
        cargo install --path crates/colony-shell
        cargo install --path crates/colony-events
        echo "Installed to $(which colony 2>/dev/null || echo '~/.cargo/bin/colony')"
        ;;
    *)
        echo "Usage: $0 [debug|release|install]"
        exit 1
        ;;
esac
