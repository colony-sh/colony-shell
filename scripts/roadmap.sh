#!/usr/bin/env bash
set -euo pipefail

COLONY_DIR="/Users/mcfearsome/src/github.com/mcfearsome/colony"
SERVER="${COLONY_DIR}/roadmap-server.mjs"

if [[ ! -f "${SERVER}" ]]; then
  echo "Error: roadmap server not found at ${SERVER}" >&2
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "Error: node is required to run the roadmap server" >&2
  exit 1
fi

echo "Starting Colony Roadmap server..."
exec node "${SERVER}"
