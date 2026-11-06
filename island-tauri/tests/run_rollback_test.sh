#!/usr/bin/env bash
# Shell-level tests for run.sh's automatic rollback, using fake `hermes-island` binaries in a
# scratch checkout. Never touches a real island. Run: island-tauri/tests/run_rollback_test.sh
set -u
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RUN_SH="$HERE/../run.sh"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/run-rollback.XXXXXX")"
trap 'rm -rf -- "$SCRATCH"' EXIT
pass=0 fail=0

# Short timings so the suite takes ~25 s.
export HERMES_ISLAND_EARLY_EXIT_SECS=3 HERMES_ISLAND_STATUS_GRACE_SECS=3 HERMES_ISLAND_GOOD_AFTER_SECS=2
unset HERMES_ISLAND_NO_SUPERVISOR WAYLAND_DISPLAY

check() { # name, condition-result (0 = ok)
  if [[ "$2" == 0 ]]; then pass=$((pass + 1)); printf 'ok   %s\n' "$1"
  else fail=$((fail + 1)); printf 'FAIL %s\n' "$1"; fi
}

new_repo() { # -> sets REPO, BIN, DATA
  REPO="$(mktemp -d "$SCRATCH/repo.XXXXXX")"
  mkdir -p "$REPO/island-tauri/src-tauri/target/release" "$REPO/.island-data-overlay"
  cp "$RUN_SH" "$REPO/island-tauri/run.sh"
  BIN="$REPO/island-tauri/src-tauri/target/release/hermes-island"
  DATA="$REPO/.island-data-overlay"
}
fake() { # path, body: a fake binary that counts its runs in $REPO/runs.<name>
  printf '#!/usr/bin/env bash\n%s\n' "$2" >"$1"
  chmod 755 "$1"
}
log_of() { cat "$DATA/monolith.log" 2>/dev/null; }
runs() { cat "$REPO/runs.$1" 2>/dev/null | wc -l; }
launch() { "$REPO/island-tauri/run.sh" --overlay "$@" >"$REPO/out" 2>"$REPO/err"; }

HEALTHY='echo run >>"$(dirname "$0")/../../../../runs.$(basename "$0")"
trap "exit 0" TERM
for i in 1 2 3 4 5 6 7 8 9 10; do printf "{\"pid\": %s}" "$$" >"$(dirname "$0")/../../../../.island-data-overlay/status.json.tmp" && mv "$(dirname "$0")/../../../../.island-data-overlay/status.json.tmp" "$(dirname "$0")/../../../../.island-data-overlay/status.json"; sleep 0.5; done'
FAIL1='echo run >>"$(dirname "$0")/../../../../runs.$(basename "$0")"; echo "boom" >&2; exit 1'

# 1. Early failure with a distinct last-good: last-good runs once, its status is the exit status.
new_repo
fake "$BIN" "$FAIL1"
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"; exit 7'
launch; code=$?
check "early failure rolls back to last-good" "$([[ $code == 7 && "$(runs last-good)" == 1 && "$(runs hermes-island)" == 1 ]] && echo 0 || echo 1)"
check "rollback is logged with the reason" "$(log_of | grep -q 'rolling back to hermes-island.last-good' && log_of | grep -q 'exited with status 1' && echo 0 || echo 1)"

# 2. No last-good: nothing to roll back to, exit status passes through, binary ran once.
new_repo
fake "$BIN" "$FAIL1"
launch; code=$?
check "no last-good: status passes through, no retry" "$([[ $code == 1 && "$(runs hermes-island)" == 1 ]] && echo 0 || echo 1)"
check "no last-good is logged" "$(log_of | grep -q 'no different last-good' && echo 0 || echo 1)"

# 3. last-good identical to the failing binary: not retried.
new_repo
fake "$BIN" "$FAIL1"
cp "$BIN" "$BIN.last-good"
launch; code=$?
check "identical last-good is not run again" "$([[ $code == 1 && "$(runs hermes-island)" == 1 && "$(runs last-good)" == 0 ]] && echo 0 || echo 1)"

# 4. Early exit 0 (user quit) never rolls back.
new_repo
fake "$BIN" 'echo run >>"$(dirname "$0")/../../../../runs.hermes-island"; exit 0'
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"'
launch; code=$?
check "early clean exit is a deliberate quit" "$([[ $code == 0 && "$(runs last-good)" == 0 ]] && echo 0 || echo 1)"

# 5. A binary that stays up but never writes a status for its pid is killed and rolled back.
new_repo
fake "$BIN" 'echo run >>"$(dirname "$0")/../../../../runs.hermes-island"; trap "exit 0" TERM; while :; do sleep 0.2; done'
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"; exit 0'
start=$SECONDS
launch; code=$?
check "silent binary is replaced by last-good" "$([[ $code == 0 && "$(runs last-good)" == 1 ]] && echo 0 || echo 1)"
check "silent binary is logged as no status" "$(log_of | grep -q 'no status.json carrying pid' && echo 0 || echo 1)"
check "silent binary was cut off by the grace period" "$([[ $((SECONDS - start)) -le 9 ]] && echo 0 || echo 1)"

# 6. A status.json that carries ANOTHER pid does not count as healthy.
new_repo
printf '{"pid": 1}' >"$DATA/status.json"
fake "$BIN" 'echo run >>"$(dirname "$0")/../../../../runs.hermes-island"; trap "exit 0" TERM; while :; do sleep 0.2; done'
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"; exit 0'
touch -d '1 hour ago' "$DATA/status.json"
launch; code=$?
check "stale status for another pid is not health" "$([[ "$(runs last-good)" == 1 ]] && echo 0 || echo 1)"

# 7. A healthy binary becomes last-good after GOOD_AFTER and its exit status passes through.
new_repo
fake "$BIN" "$HEALTHY"
launch; code=$?
check "healthy binary is saved as last-good" "$([[ -x "$BIN.last-good" ]] && cmp -s "$BIN" "$BIN.last-good" && echo 0 || echo 1)"
check "healthy run is not rolled back" "$([[ $code == 0 && "$(runs hermes-island)" == 1 ]] && echo 0 || echo 1)"
check "saving last-good is logged" "$(log_of | grep -q 'saved hermes-island.last-good' && echo 0 || echo 1)"
check "no temp copies left behind" "$([[ -z "$(ls "$BIN".last-good.new.* 2>/dev/null)" ]] && echo 0 || echo 1)"

# 8. Both binaries fail: exactly one attempt each, then it stops (no loop).
new_repo
fake "$BIN" "$FAIL1"
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"; exit 9'
launch; code=$?
check "failing last-good ends the sequence" "$([[ $code == 9 && "$(runs hermes-island)" == 1 && "$(runs last-good)" == 1 ]] && echo 0 || echo 1)"

# 9. SIGTERM to run.sh reaches the island and is never mistaken for a failure.
new_repo
fake "$BIN" "$HEALTHY"
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"'
"$REPO/island-tauri/run.sh" --overlay >"$REPO/out" 2>"$REPO/err" &
sup=$!
sleep 1
kill -TERM "$sup"
wait "$sup"; code=$?
check "SIGTERM is forwarded and not rolled back" "$([[ $code == 0 && "$(runs last-good)" == 0 && "$(runs hermes-island)" == 1 ]] && echo 0 || echo 1)"

# 10. A live island already owns the data dir: plain exec, no supervision, no rollback.
new_repo
printf '{"pid": %s}' "$$" >"$DATA/status.json"
fake "$BIN" "$FAIL1"
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"'
launch; code=$?
check "second launch while an island is alive just execs" "$([[ $code == 1 && "$(runs last-good)" == 0 ]] && echo 0 || echo 1)"

# 11. Harness launches (--test-instance, no --overlay) and the opt-out are never supervised.
new_repo
fake "$BIN" "$FAIL1"
fake "$BIN.last-good" 'echo run >>"$(dirname "$0")/../../../../runs.last-good"'
"$REPO/island-tauri/run.sh" --overlay --test-instance >/dev/null 2>&1; a=$?
"$REPO/island-tauri/run.sh" --data-dir /x >/dev/null 2>&1; b=$?
HERMES_ISLAND_NO_SUPERVISOR=1 "$REPO/island-tauri/run.sh" --overlay >/dev/null 2>&1; c=$?
check "test-instance / harness / opt-out keep exec semantics" "$([[ $a == 1 && $b == 1 && $c == 1 && "$(runs last-good)" == 0 ]] && echo 0 || echo 1)"

printf '%s passed, %s failed\n' "$pass" "$fail"
[[ "$fail" == 0 ]]
