#!/usr/bin/env bash
# Deploy a UI-only change to the running island WITHOUT a Rust rebuild or a restart.
#
#   scripts/deploy-ui.sh            build the UI, swap dist/, tell the island to reload
#   scripts/deploy-ui.sh --no-reload  build + swap only (the island picks dist/ up at its next start)
#
# How it works: a release island serves island-tauri/dist from disk when dist/'s manifest
# verifies (see src-tauri/src/ui_assets.rs). This script builds into a staging directory first (a
# failed build leaves the working dist/ alone), swaps it in, then sends `reload-ui` on the
# island's panel socket; the island re-verifies dist/ and reloads only its webview. It never
# restarts or signals the island.
#
# It refuses ("needs ./build.sh") when the UI was written against a different IPC contract
# (ui/ipc/bindings.ts) than the installed binary: the binary would never serve it. The running
# island double-checks the same thing and refuses too.
#
# Environment (for tests): HERMES_ISLAND_BIN, HERMES_ISLAND_DATA (data dir holding panel.sock).
set -euo pipefail
APP="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$(cd -- "$APP/.." && pwd)"
BIN="${HERMES_ISLAND_BIN:-$APP/src-tauri/target/release/hermes-island}"
DATA="${HERMES_ISLAND_DATA:-$ROOT/.island-data-overlay}"
reload=1
case "${1-}" in
  '') ;;
  --no-reload) reload=0 ;;
  *) printf 'Usage: %s [--no-reload]\n' "$0" >&2; exit 2 ;;
esac
cd -- "$APP"

die() { printf 'deploy-ui: %s\n' "$*" >&2; exit 1; }
needs_build() {
  printf 'deploy-ui: needs ./build.sh (%s)\n' "$*" >&2
  exit 3
}

[[ -x "$BIN" ]] || needs_build "no release binary at $BIN"
want=$(node scripts/write-ui-manifest.mjs --check)
have=$("$BIN" --print-contract-hash 2>/dev/null) || needs_build "$BIN does not report a contract hash (built before disk-UI support)"
[[ "$want" == "$have" ]] || needs_build "bindings.ts is ${want:0:12}, the binary was built with ${have:0:12}"

stage=".dist-staging"
rm -rf -- "$stage" dist.old
trap 'rm -rf -- "$stage" dist.old' EXIT
# Same dependencies and flags as build.sh's UI step; never npm install.
npm run -s build:ui -- --outDir "$stage" --emptyOutDir
[[ -f "$stage/.monolith-manifest.json" ]] || die "the build wrote no manifest"
built=$(node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).contractHash)' "$stage/.monolith-manifest.json")
[[ "$built" == "$have" ]] || needs_build "built manifest contract ${built:0:12} != binary ${have:0:12}"

# Swap. Two renames: the window with no dist/ is microseconds, and the running island keeps
# its verified copy in memory regardless.
[[ ! -e dist ]] || mv -- dist dist.old
mv -- "$stage" dist
printf 'deploy-ui: dist/ updated (%s files)\n' "$(node -e 'process.stdout.write(String(Object.keys(JSON.parse(require("fs").readFileSync("dist/.monolith-manifest.json","utf8")).files).length))')"

[[ "$reload" == 1 ]] || exit 0
sock="$DATA/panel.sock"
if [[ ! -S "$sock" ]]; then
  printf 'deploy-ui: no running island (%s); it will use dist/ at its next start\n' "$sock"
  exit 0
fi
reply=$(python3 -I -S -c '
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.settimeout(10)
s.connect(sys.argv[1])
s.sendall(b"reload-ui\n")
s.shutdown(socket.SHUT_WR)
data = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    data += chunk
sys.stdout.write(data.decode())
' "$sock") || die "could not reach the island on $sock"
printf 'deploy-ui: island replied %s\n' "$reply"
case "$reply" in
  *'"ok":true'*) ;;
  *'"needsBuild":true'*) needs_build "the running island refused: contract mismatch" ;;
  *) die "the island did not reload (see its log)" ;;
esac
