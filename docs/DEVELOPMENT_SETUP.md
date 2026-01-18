# Development Setup for Colony Projects

This guide covers setting up both `colony-shell` and `colony-terminal` for local development on a new machine.

## Prerequisites

- **Rust** (1.75+): Install via [rustup](https://rustup.rs/)
- **Git**: For cloning repositories
- **macOS** (primary), Linux (supported), Windows (via WSL)

## Repository Structure

The Colony ecosystem consists of two co-dependent repositories that must be cloned as siblings:

```
~/src/github.com/mcfearsome/
├── colony-shell/      # Shell interpreter (this repo)
└── colony-terminal/   # GPU-accelerated terminal emulator
```

**Important**: `colony-terminal` depends on `colony-protocol` from `colony-shell` via a relative path. The repositories must be siblings in the same parent directory.

## Quick Setup

```bash
# Create parent directory
mkdir -p ~/src/github.com/mcfearsome
cd ~/src/github.com/mcfearsome

# Clone both repositories
git clone git@github.com:mcfearsome/colony-shell.git
git clone git@github.com:mcfearsome/colony-terminal.git

# Build colony-shell first (provides colony-protocol dependency)
cd colony-shell
cargo build --release

# Build colony-terminal
cd ../colony-terminal
cargo build --release
```

## Detailed Setup

### 1. Clone Repositories

```bash
cd ~/src/github.com/mcfearsome  # or your preferred location

git clone git@github.com:mcfearsome/colony-shell.git
git clone git@github.com:mcfearsome/colony-terminal.git
```

### 2. Build Colony Shell

```bash
cd colony-shell
cargo build --release
```

The shell binary will be at `target/release/colony`.

To install system-wide (optional):

```bash
cargo install --path crates/colony-shell
```

### 3. Build Colony Terminal

```bash
cd ../colony-terminal
cargo build --release
```

The terminal binary will be at `target/release/colony-terminal`.

### 4. Run Together

```bash
# Option 1: Run terminal with explicit shell path
./target/release/colony-terminal --shell ../colony-shell/target/release/colony

# Option 2: Add colony to PATH, terminal will find it automatically
export PATH="$HOME/src/github.com/mcfearsome/colony-shell/target/release:$PATH"
./target/release/colony-terminal
```

## Development Workflow

### Working on Colony Shell

```bash
cd colony-shell
cargo build          # Debug build
cargo test           # Run tests
cargo run            # Run shell directly
```

### Working on Colony Terminal

```bash
cd colony-terminal
cargo build          # Debug build
cargo run            # Run terminal (uses configured shell)
```

### Cross-Project Changes

When modifying `colony-protocol` (shared types):

1. Make changes in `colony-shell/crates/colony-protocol/`
2. Rebuild colony-shell: `cargo build -p colony-protocol`
3. Rebuild colony-terminal: `cd ../colony-terminal && cargo build`

Both projects will use the updated protocol types.

## Project Dependencies

```
colony-terminal
├── colony-term-core
│   └── colony-protocol (from ../colony-shell/crates/colony-protocol)
├── colony-term-render
├── colony-term-ui
├── colony-term-theme
└── colony-term-app

colony-shell
├── colony-parser
├── colony-expand
├── colony-exec
├── colony-protocol    ← Shared with colony-terminal
├── colony-agent
├── colony-channels
├── colony-config
├── colony-memory
├── colony-tuple
├── colony-mcp
├── colony-focus
├── colony-secure
├── colony-tree-sitter
└── colony-ui-core
```

## Troubleshooting

### "colony-protocol not found"

Ensure both repositories are siblings in the same parent directory:

```bash
ls -la ..
# Should show both colony-shell/ and colony-terminal/
```

### Build Fails with Missing Dependencies

On macOS, ensure Xcode Command Line Tools are installed:

```bash
xcode-select --install
```

On Linux, install build essentials:

```bash
# Ubuntu/Debian
sudo apt install build-essential pkg-config

# Fedora
sudo dnf install gcc pkg-config
```

### SHELL Environment Variable Shows Wrong Shell

Colony Shell 0.4.1+ correctly sets `SHELL` to its own path. If running an older version, upgrade:

```bash
cd colony-shell
git pull
cargo build --release
```

## IDE Setup

### VS Code

Recommended extensions:
- rust-analyzer
- Even Better TOML
- crates

### RustRover / IntelliJ

Open both projects in separate windows, or create a workspace that includes both.

## Running Tests

```bash
# Colony Shell tests
cd colony-shell
cargo test

# Colony Terminal tests  
cd ../colony-terminal
cargo test
```

## Release Builds

```bash
# Optimized release builds
cd colony-shell && cargo build --release
cd ../colony-terminal && cargo build --release
```

Release binaries are in `target/release/` for each project.
