#!/usr/bin/env bash
# Build assets with the installed lockfile-resolved dependencies; never runs npm ci.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$ROOT"
npm run build:ui
# Cargo does not track changes in frontendDist itself.
touch src-tauri/build.rs
cargo build --manifest-path src-tauri/Cargo.toml --release --locked --features custom-protocol
