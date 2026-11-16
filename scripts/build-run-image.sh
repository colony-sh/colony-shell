#!/usr/bin/env bash
# Build the image a named agent's runs execute in: localhost/colony-run:latest.
#
# It is rust:1-bookworm with this checkout's colony and colony-events, built for Linux in
# that same image, in /usr/local/bin, and an empty /work for the run's scratch worktree.
#
# Usage: scripts/build-run-image.sh
#
# Contacts docker.io (the base image, first run only) and crates.io (crates the Mac build
# has not fetched). The build output and the crate cache live in two podman volumes, so a
# second build only recompiles what changed.
# ponytail: the run image is the 1.5GB build image; base it on bookworm-slim (plus libssl and CA
# certificates) when its size matters.
set -euo pipefail

BASE="docker.io/library/rust:1-bookworm"
IMAGE="localhost/colony-run:latest"
SRC="$(cd "$(dirname "$0")/.." && pwd)"

podman volume exists colony-linux-target || podman volume create colony-linux-target >/dev/null
podman volume exists colony-cargo-registry || podman volume create colony-cargo-registry >/dev/null

echo "Building colony and colony-events for Linux in $BASE..."
podman run --rm \
    -v "$SRC":/src:ro \
    -v colony-linux-target:/target \
    -v colony-cargo-registry:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target \
    -e CARGO_PROFILE_RELEASE_LTO=false \
    -e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    -w /src \
    "$BASE" \
    cargo build --release --locked -j 4 --bin colony --bin colony-events

echo "Making $IMAGE..."
name="colony-run-build-$$"
podman run --name "$name" -v colony-linux-target:/target:ro "$BASE" \
    sh -c 'install -m 755 /target/release/colony /target/release/colony-events /usr/local/bin/ && mkdir -p /work'
podman commit -q \
    --change 'WORKDIR /work' \
    --change 'CMD ["sleep", "infinity"]' \
    --change 'ENTRYPOINT []' \
    "$name" "$IMAGE" >/dev/null
podman rm "$name" >/dev/null
podman run --rm "$IMAGE" colony -c 'echo colony runs in the image'
