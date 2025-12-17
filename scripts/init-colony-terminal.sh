#!/bin/bash
# Initialize colony-terminal repository and start Claude with the init prompt

set -e

INIT_PROMPT="/Users/mcfearsome/src/github.com/mcfearsome/colony-shell/docs/COLONY_TERMINAL_INIT.md"
TARGET_DIR="$HOME/src/github.com/mcfearsome"
PROJECT_NAME="colony-terminal"

# Verify init prompt exists
if [[ ! -f "$INIT_PROMPT" ]]; then
    echo "Error: Init prompt not found at $INIT_PROMPT"
    exit 1
fi

# Navigate to target directory
echo "Navigating to $TARGET_DIR..."
cd "$TARGET_DIR"

# Create project directory
if [[ -d "$PROJECT_NAME" ]]; then
    echo "Warning: $PROJECT_NAME already exists"
    read -p "Delete and recreate? (y/N): " confirm
    if [[ "$confirm" == "y" || "$confirm" == "Y" ]]; then
        rm -rf "$PROJECT_NAME"
        mkdir "$PROJECT_NAME"
    fi
else
    mkdir "$PROJECT_NAME"
    echo "Created $PROJECT_NAME directory"
fi

# Enter project directory
cd "$PROJECT_NAME"
echo "Working directory: $(pwd)"

# Initialize git repo
if [[ ! -d ".git" ]]; then
    git init
    echo "Initialized git repository"
fi

# Start Claude with the init prompt
echo ""
echo "Starting Claude Code with colony-terminal init prompt..."
echo "=============================================="
echo ""

# Use claude with the init prompt as the first message
# The --print flag reads a file and includes it in context
exec claude --system-prompt "$INIT_PROMPT" "Please read the initialization prompt above and set up the colony-terminal project. Start by creating the directory structure and system files (.claude/system/, CLAUDE.md, Cargo.toml, etc.)."
