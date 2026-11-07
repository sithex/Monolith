#!/usr/bin/env bash
# Install the habby-metrics Hermes plugin (per-user, Linux).
#
#   1. Backs up <HERMES_HOME>/config.yaml to <HERMES_HOME>/backups/config.yaml.<YYYYmmdd-HHMMSS>
#   2. Copies plugin.yaml, __init__.py and README.md to <HERMES_HOME>/plugins/habby-metrics
#      (Hermes's user plugin dir, scanned by hermes_cli/plugins.py discover_plugins())
#   3. Runs `hermes plugins enable habby-metrics` (adds it to plugins.enabled in config.yaml)
#   4. Verifies with Hermes's own discovery: tests/verify_home.py (listed AND enabled) and
#      `hermes plugins list --user --json`.
#
# Changes nothing unless --yes is given. Running Hermes processes keep their current plugins until they
# restart; new sessions load it. On the default home `hermes plugins enable` also asks a running messaging
# gateway to re-discover plugins (no restart), so that gateway may load it immediately.
#
#   ./install.sh            # dry run: show what would happen
#   ./install.sh --yes      # do it
#   ./install.sh --yes --hermes-home /tmp/scratch-home   # another home (hermes child isolated from the live gateway)
set -euo pipefail

PLUGIN=habby-metrics
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
YES=0
HOME_ARG=""
HERMES_BIN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --yes|-y) YES=1 ;;
    --hermes-home) HOME_ARG="${2:?--hermes-home needs a dir}"; shift ;;
    --hermes) HERMES_BIN="${2:?--hermes needs a path}"; shift ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 64 ;;
  esac
  shift
done

fail() { echo "ERROR: $*" >&2; exit 1; }

NATIVE_HOME="$HOME/.hermes"
TARGET="${HOME_ARG:-${HERMES_HOME:-$NATIVE_HOME}}"
TARGET="$(realpath -m "$TARGET")"
IS_DEFAULT=0
[ "$TARGET" = "$(realpath -m "$NATIVE_HOME")" ] && IS_DEFAULT=1
DEST="$TARGET/plugins/$PLUGIN"
CONFIG="$TARGET/config.yaml"
BACKUP="$TARGET/backups/config.yaml.$(date +%Y%m%d-%H%M%S)"

echo "habby-metrics installer"
echo "  Hermes home : $TARGET $([ $IS_DEFAULT = 1 ] && echo '(default / live)' || echo '(non-default: isolated hermes child)')"
echo "  backup      : $CONFIG -> $BACKUP"
echo "  copy        : $SRC -> $DEST  (plugin.yaml, __init__.py, README.md)"
echo "  enable      : hermes plugins enable $PLUGIN   (edits plugins.enabled in config.yaml)"
echo "  verify      : tests/verify_home.py + hermes plugins list --user --json"
echo "  note        : running Hermes processes keep their plugins until restarted; new sessions load it"
if [ "$YES" != 1 ]; then
  echo
  echo "Refusing to run without --yes. Nothing was changed."
  exit 2
fi

for f in plugin.yaml __init__.py; do [ -f "$SRC/$f" ] || fail "missing $f next to install.sh ($SRC)"; done
[ -f "$CONFIG" ] || fail "no config.yaml in $TARGET (is this a Hermes home?)"
if [ -z "$HERMES_BIN" ]; then
  HERMES_BIN="$(command -v hermes || true)"
  [ -n "$HERMES_BIN" ] || fail "hermes not on PATH; pass --hermes /path/to/hermes"
fi
PY="$(ls -d "$NATIVE_HOME"/tools/python-*/bin/python3 2>/dev/null | sort -V | tail -1 || true)"

# The hermes child: drop inherited Hermes session env (this may run from a terminal inside Hermes),
# then point at the target home.
while IFS= read -r v; do unset "$v"; done < <(compgen -e | grep -E '^(HERMES_|_HERMES)' || true)
export HERMES_HOME="$TARGET"
export PYTHONIOENCODING=utf-8
if [ "$IS_DEFAULT" != 1 ]; then
  # Not the live home: keep `plugins enable` from nudging the live gateway / dashboard.
  export HERMES_DATA_DIR_SUFFIX=-habby-isolated
  export HERMES_GATEWAY_LOCK_DIR="$TARGET/locks"
fi

# 1) backup
mkdir -p "$(dirname "$BACKUP")"
cp -p "$CONFIG" "$BACKUP"
echo "backed up config.yaml -> $BACKUP"

# 2) copy (replace an older copy; never the tests or these scripts)
rm -rf "$DEST"
mkdir -p "$DEST"
for f in plugin.yaml __init__.py README.md; do if [ -f "$SRC/$f" ]; then cp "$SRC/$f" "$DEST/"; fi; done
echo "copied plugin -> $DEST"

# 3) enable
"$HERMES_BIN" plugins enable "$PLUGIN" || fail "hermes plugins enable failed (config backup: $BACKUP)"

# 4) verify with Hermes itself (a regex over config.yaml would also match plugins.disabled)
verified=0
if [ -n "$PY" ] && [ -f "$SRC/tests/verify_home.py" ]; then
  if out="$("$PY" -I "$SRC/tests/verify_home.py" "$TARGET" --allow-live 2>&1)"; then
    verified=1; echo "verified (Hermes discover_plugins): $out"
  else
    echo "verify_home.py: $out" >&2
  fi
fi
list="$("$HERMES_BIN" plugins list --user --json 2>/dev/null || true)"
status="$(printf "%s" "$list" | "${PY:-python3}" -c 'import json,sys
try: print(next((p.get("status") for p in json.load(sys.stdin) if p.get("name") == "habby-metrics"), "missing"))
except Exception: print("unparsable")' 2>/dev/null || echo "?")"
echo "hermes plugins list: $PLUGIN -> $status"
[ "$status" = enabled ] && verified=1
[ "$verified" = 1 ] || fail "could not verify that $PLUGIN is enabled (backup: $BACKUP)"

echo
echo "habby-metrics installed and enabled in $CONFIG"
echo "New Hermes sessions load it; running ones keep their current plugins until restarted."
echo "Files appear in $TARGET/cache/habby-metrics after the first agent turn (read by Monolith's AI Agents tab)."
echo "Plan windows: Hermes's in-memory cache only. Network fetching is disabled in this Monolith port;"
echo "HABBY_METRICS_PREVIEW=0 drops tool previews/goals."
echo "Undo: ./uninstall.sh --yes   (or restore $BACKUP)"
