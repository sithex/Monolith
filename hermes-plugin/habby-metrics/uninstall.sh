#!/usr/bin/env bash
# Uninstall the habby-metrics Hermes plugin (per-user, Linux).
#
#   1. Backs up <HERMES_HOME>/config.yaml to <HERMES_HOME>/backups/config.yaml.<YYYYmmdd-HHMMSS>
#   2. Runs `hermes plugins disable habby-metrics` (while the folder still exists, so the key resolves)
#   3. Removes <HERMES_HOME>/plugins/habby-metrics and <HERMES_HOME>/cache/habby-metrics
#
# Changes nothing unless --yes is given. Backups are kept. Running Hermes processes keep the plugin loaded
# (and keep re-creating cache/habby-metrics/snapshot-<pid>.json) until they exit.
#
#   ./uninstall.sh --yes [--hermes-home DIR] [--hermes /path/to/hermes]
set -euo pipefail

PLUGIN=habby-metrics
YES=0
HOME_ARG=""
HERMES_BIN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --yes|-y) YES=1 ;;
    --hermes-home) HOME_ARG="${2:?--hermes-home needs a dir}"; shift ;;
    --hermes) HERMES_BIN="${2:?--hermes needs a path}"; shift ;;
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 64 ;;
  esac
  shift
done

fail() { echo "ERROR: $*" >&2; exit 1; }

NATIVE_HOME="$HOME/.hermes"
TARGET="$(realpath -m "${HOME_ARG:-${HERMES_HOME:-$NATIVE_HOME}}")"
IS_DEFAULT=0
[ "$TARGET" = "$(realpath -m "$NATIVE_HOME")" ] && IS_DEFAULT=1
DEST="$TARGET/plugins/$PLUGIN"
CACHE="$TARGET/cache/$PLUGIN"
CONFIG="$TARGET/config.yaml"
BACKUP="$TARGET/backups/config.yaml.$(date +%Y%m%d-%H%M%S)"

echo "habby-metrics uninstaller"
echo "  Hermes home : $TARGET $([ $IS_DEFAULT = 1 ] && echo '(default / live)' || echo '(non-default: isolated hermes child)')"
echo "  backup      : $CONFIG -> $BACKUP"
echo "  disable     : hermes plugins disable $PLUGIN"
echo "  remove      : $DEST"
echo "  remove      : $CACHE"
echo "  note        : running Hermes processes keep the plugin (and re-create the cache dir) until restarted"
if [ "$YES" != 1 ]; then
  echo
  echo "Refusing to run without --yes. Nothing was changed."
  exit 2
fi
[ -f "$CONFIG" ] || fail "no config.yaml in $TARGET (is this a Hermes home?)"
if [ -z "$HERMES_BIN" ]; then
  HERMES_BIN="$(command -v hermes || true)"
  [ -n "$HERMES_BIN" ] || fail "hermes not on PATH; pass --hermes /path/to/hermes"
fi
while IFS= read -r v; do unset "$v"; done < <(compgen -e | grep -E '^(HERMES_|_HERMES)' || true)
export HERMES_HOME="$TARGET"
export PYTHONIOENCODING=utf-8
if [ "$IS_DEFAULT" != 1 ]; then
  export HERMES_DATA_DIR_SUFFIX=-habby-isolated
  export HERMES_GATEWAY_LOCK_DIR="$TARGET/locks"
fi

mkdir -p "$(dirname "$BACKUP")"
cp -p "$CONFIG" "$BACKUP"
echo "backed up config.yaml -> $BACKUP"

if [ -d "$DEST" ]; then
  "$HERMES_BIN" plugins disable "$PLUGIN" || echo "warning: hermes plugins disable failed (continuing)" >&2
else
  echo "plugin folder not present; skipping disable"
fi
for p in "$DEST" "$CACHE"; do
  if [ -e "$p" ]; then rm -rf "$p"; echo "removed $p"; fi
done
echo
echo "habby-metrics removed. Running Hermes processes drop it when they restart;"
echo "delete $CACHE again afterwards if it came back."
