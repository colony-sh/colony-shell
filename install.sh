#!/bin/sh
# Colony Shell installer
# Usage: curl -fsSL https://colony.sh/install | sh
#    or: ./install.sh [--no-plugin]
set -eu

COLONY_VERSION="0.4.1"
INSTALL_DIR="${COLONY_INSTALL_DIR:-$HOME/.local/bin}"
PLUGIN_DIR="$HOME/.claude/plugins/colony"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
DIM='\033[2m'
RESET='\033[0m'

info()  { printf "${CYAN}▸${RESET} %s\n" "$1"; }
ok()    { printf "${GREEN}✓${RESET} %s\n" "$1"; }
err()   { printf "${RED}✗${RESET} %s\n" "$1" >&2; }

skip_plugin=false
from_source=false

for arg in "$@"; do
    case "$arg" in
        --no-plugin)  skip_plugin=true ;;
        --from-source) from_source=true ;;
        --help|-h)
            echo "Colony Shell Installer"
            echo ""
            echo "Usage: install.sh [options]"
            echo ""
            echo "Options:"
            echo "  --no-plugin     Skip Claude Code plugin installation"
            echo "  --from-source   Build from source (requires Rust toolchain)"
            echo "  --help          Show this help"
            exit 0
            ;;
        *) err "unknown option: $arg"; exit 1 ;;
    esac
done

# Detect platform
detect_platform() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os" in
        Darwin)
            case "$arch" in
                arm64)  echo "aarch64-apple-darwin" ;;
                x86_64) echo "x86_64-apple-darwin" ;;
                *)      err "unsupported architecture: $arch"; exit 1 ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64)  echo "x86_64-unknown-linux-gnu" ;;
                aarch64) echo "aarch64-unknown-linux-gnu" ;;
                *)       err "unsupported architecture: $arch"; exit 1 ;;
            esac
            ;;
        *)
            err "unsupported OS: $os"
            exit 1
            ;;
    esac
}

# Build from source
install_from_source() {
    info "Building Colony Shell from source..."

    if ! command -v cargo >/dev/null 2>&1; then
        err "cargo not found. Install Rust: https://rustup.rs"
        exit 1
    fi

    # Find the repo root (works whether run from repo or via curl)
    if [ -f "Cargo.toml" ] && grep -q "colony-shell" Cargo.toml 2>/dev/null; then
        repo_dir="."
    elif [ -f "$HOME/src/github.com/colony-sh/colony-shell/Cargo.toml" ]; then
        repo_dir="$HOME/src/github.com/colony-sh/colony-shell"
    else
        err "Colony Shell source not found. Clone it first or run from the repo directory."
        exit 1
    fi

    (cd "$repo_dir" && cargo build --release -p colony-shell)

    mkdir -p "$INSTALL_DIR"
    cp "$repo_dir/target/release/colony" "$INSTALL_DIR/colony"
    chmod +x "$INSTALL_DIR/colony"

    ok "Built and installed colony to $INSTALL_DIR/colony"
}

# Download prebuilt binary (future — for now, source only)
install_prebuilt() {
    platform=$(detect_platform)
    url="https://github.com/colony-sh/colony-shell/releases/download/v${COLONY_VERSION}/colony-${platform}.tar.gz"

    info "Downloading Colony Shell v${COLONY_VERSION} for ${platform}..."

    tmp=$(mktemp -d)
    if curl --retry 3 -fsSL "$url" -o "$tmp/colony.tar.gz" 2>/dev/null; then
        mkdir -p "$INSTALL_DIR"
        tar xzf "$tmp/colony.tar.gz" -C "$INSTALL_DIR"
        chmod +x "$INSTALL_DIR/colony"
        rm -rf "$tmp"
        ok "Installed colony to $INSTALL_DIR/colony"
    else
        info "No prebuilt binary available, building from source..."
        rm -rf "$tmp"
        install_from_source
    fi
}

# Install Claude Code plugin
install_plugin() {
    if [ "$skip_plugin" = true ]; then
        info "Skipping Claude Code plugin (--no-plugin)"
        return
    fi

    if ! command -v claude >/dev/null 2>&1; then
        info "Claude Code not found — skipping plugin install"
        info "Install later: colony plugin install"
        return
    fi

    info "Installing Colony Claude Code plugin..."

    # Find plugin source
    plugin_src=""
    if [ -d "claude-plugin/.claude-plugin" ]; then
        plugin_src="claude-plugin"
    elif [ -d "$HOME/src/github.com/colony-sh/colony-shell/claude-plugin/.claude-plugin" ]; then
        plugin_src="$HOME/src/github.com/colony-sh/colony-shell/claude-plugin"
    fi

    if [ -z "$plugin_src" ]; then
        info "Plugin source not found — skipping"
        return
    fi

    # Copy plugin files
    mkdir -p "$PLUGIN_DIR"
    rm -rf "${PLUGIN_DIR:?}/.claude-plugin" "${PLUGIN_DIR:?}/skills" "${PLUGIN_DIR:?}/commands" "${PLUGIN_DIR:?}/hooks"
    cp -r "$plugin_src/.claude-plugin" "$PLUGIN_DIR/"
    cp -r "$plugin_src/skills" "$PLUGIN_DIR/"
    cp -r "$plugin_src/commands" "$PLUGIN_DIR/"
    cp -r "$plugin_src/hooks" "$PLUGIN_DIR/"

    ok "Installed Colony plugin to $PLUGIN_DIR"
    printf "${DIM}  Skills: colony-shell-awareness, colony-channel-awareness, colony-canvas-awareness, colony-whiteboard${RESET}\n"
    printf "${DIM}  Commands: /colony-status, /colony-context${RESET}\n"
}

# Check PATH
check_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo ""
            info "Add to your shell profile:"
            printf "    export PATH=\"%s:\$PATH\"\n" "$INSTALL_DIR"
            ;;
    esac
}

# Main
main() {
    echo ""
    printf "${CYAN}Colony Shell${RESET} v${COLONY_VERSION}\n"
    echo ""

    if [ "$from_source" = true ]; then
        install_from_source
    else
        install_prebuilt
    fi

    install_plugin
    check_path

    echo ""
    ok "Colony Shell installed successfully"
    printf "${DIM}  Run: colony${RESET}\n"
}

main
