#!/usr/bin/env bash
# One-command local pre-push check. Runs every gate, prints a PASS/FAIL table, exits non-zero
# when anything failed. Runs at low CPU/IO priority by default (it is meant to be run while you
# work); the full log of each step goes to a temp directory whose path is printed on failure.
#
#   scripts/precheck.sh                run everything
#   scripts/precheck.sh --fast         skip the Rust steps (UI / QML / secrets / launcher only)
#   scripts/precheck.sh --skip a,b     skip steps by key (first column of the table)
#   scripts/precheck.sh --full-speed   do not lower priority
#   scripts/precheck.sh --keep-logs    keep the log directory even when everything passes
#
# Steps: rust-test (lib+units+services), clippy (-D warnings, lib+tests), bindings, tsc,
# test-ui, qml (offscreen qmltestrunner), launcher (run.sh rollback + launch tests),
# py-tests (island/tests), secrets (whole tree). Not covered: the nested-compositor GUI suites.
set -uo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/island-tauri"
fast=0 nice=1 keep=0 skip=","
while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast) fast=1 ;;
    --full-speed) nice=0 ;;
    --keep-logs) keep=1 ;;
    --skip) shift; skip=",${1-}," ;;
    --skip=*) skip=",${1#--skip=}," ;;
    -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
    *) printf 'precheck: unknown option %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done
if [[ "$fast" == 1 ]]; then skip="${skip}rust-test,clippy,bindings,"; fi

prefix=() scope=()
if [[ "$nice" == 1 ]]; then
  prefix=(nice -n 15)
  command -v ionice >/dev/null 2>&1 && prefix=(nice -n 15 ionice -c3)
  # A low-weight, memory-capped scope when systemd user scopes work.
  if command -v systemd-run >/dev/null 2>&1 && systemd-run --user --scope -q true 2>/dev/null; then
    scope=(systemd-run --user --scope -q -p CPUWeight=20 -p IOWeight=20 -p MemoryMax=6G)
  fi
fi
logs="$(mktemp -d "${TMPDIR:-/tmp}/precheck.XXXXXX")"
keys=() results=() secs=()
failed=0

step() { # key, description, command...
  local key="$1" desc="$2" start rc=0
  shift 2
  keys+=("$key")
  if [[ "$skip" == *",$key,"* ]]; then
    results+=("SKIP")
    secs+=("-")
    return 0
  fi
  printf '... %-10s %s\n' "$key" "$desc" >&2
  start=$SECONDS
  "$@" >"$logs/$key.log" 2>&1 || rc=$?
  secs+=("$((SECONDS - start))s")
  if [[ "$rc" == 0 ]]; then
    results+=("PASS")
  else
    results+=("FAIL")
    failed=1
  fi
}
run() { "${scope[@]}" "${prefix[@]}" "$@"; }
in_dir() { local d="$1"; shift; (cd -- "$d" && "$@"); }

step rust-test "cargo test --lib --test units --test services" \
  in_dir "$APP/src-tauri" run cargo test --locked --no-fail-fast --lib --test units --test services
# Clippy is warning-free on main, so any warning is new and fails the gate.
step clippy "cargo clippy --lib --tests -D warnings" \
  in_dir "$APP/src-tauri" run cargo clippy --locked --lib --tests -- -D warnings
step bindings "check-bindings (bindings.ts current, no split types)" \
  in_dir "$APP" run node scripts/check-bindings.mjs
step tsc "tsc --noEmit" in_dir "$APP" run npx tsc --noEmit -p .
step test-ui "npm run test:ui" in_dir "$APP" run npm run -s test:ui -- --test-reporter=dot
qml_run() {
  local runner
  runner=$(command -v qmltestrunner || ls /usr/lib/qt6/bin/qmltestrunner 2>/dev/null) || { echo "qmltestrunner not found"; return 1; }
  QT_QPA_PLATFORM=offscreen run "$runner" -platform offscreen -input "$ROOT/island/qmltests"
}
step qml "qmltestrunner (offscreen) island/qmltests" in_dir "$ROOT" qml_run
step launcher "run.sh rollback + launcher plumbing tests" \
  in_dir "$APP" bash -c 'tests/run_rollback_test.sh && python3 -m unittest tests/live_launch_test.py'
step py-tests "island/tests (python)" in_dir "$ROOT" run python3 -m unittest discover -s island/tests -p 'test_*.py'
step secrets "scripts/check-secrets.sh --all" in_dir "$ROOT" bash scripts/check-secrets.sh --all

printf '\n%-11s %-6s %s\n' STEP RESULT TIME
for i in "${!keys[@]}"; do printf '%-11s %-6s %s\n' "${keys[$i]}" "${results[$i]}" "${secs[$i]}"; done
if [[ "$failed" == 1 ]]; then
  printf '\nprecheck: FAIL. Tail of each failing step:\n'
  for i in "${!keys[@]}"; do
    if [[ "${results[$i]}" == FAIL ]]; then
      printf '\n--- %s (%s) ---\n' "${keys[$i]}" "$logs/${keys[$i]}.log"
      tail -n 15 -- "$logs/${keys[$i]}.log"
    fi
  done
  printf '\nFull logs: %s\n' "$logs"
  exit 1
fi
printf '\nprecheck: PASS\n'
if [[ "$keep" == 1 ]]; then printf 'Logs kept in %s\n' "$logs"; else rm -rf -- "$logs"; fi
