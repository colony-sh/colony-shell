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
        cargo build --bin colony
        echo "Binary: target/debug/colony"
        ;;
    release)
        echo "Building colony (release)..."
        cargo build --bin colony --release
        echo "Binary: target/release/colony"
        ;;
    install)
        echo "Installing colony..."
        cargo install --path crates/colony-shell
        echo "Installed to $(which colony 2>/dev/null || echo '~/.cargo/bin/colony')"
        ;;
    *)
        echo "Usage: $0 [debug|release|install]"
        exit 1
        ;;
esac
