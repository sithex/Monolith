#!/usr/bin/env bash
# Phase 3 gate (SPEC §8 Phase 3, revised after the Phase 3 review). Runs, in order:
#   1. earlier gates: Phase 1 and 1.5 (unless SKIP_EARLIER=1), then Phase 2.5 (which itself runs the
#      Phase 2 gate: fmt, clippy -D warnings, ALL cargo tests, typecheck/build, npm audit, bindings
#      regenerate+check, legacy byte-identity with the pinned renderer patch, dependency decisions,
#      leak audit). CLEAN=1 is passed through for a from-scratch build.
#   2. the Phase 3 acceptance: required named tests (reducer hover sources, reclaim + every refusal
#      case, rename-based status, actor-rev geometry, capacity), the node store tests, both
#      nested-KWin scenarios (host + actor) on their own with output kept as evidence, renderer
#      patch equality and absence of renderer-side state/timers, settled-screenshot comparison with
#      the Phase 2 baseline, and a FRESH mutation summary (>= 13 named failures, all restored).
#   3. its own before/after audit: live island PID/start/exe, protected legacy trees, X sockets,
#      scratch dirs, leaked processes (including the Phase 3 EIS helper), real-home markers.
# Usage: island-tauri/scripts/phase3-gate.sh   (CLEAN=1, SKIP_EARLIER=1 optional)
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(cd "$ROOT/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ROOT"
EVID="$ROOT/evidence/phase3"
mkdir -p "$EVID"

if [[ $# -ne 0 ]]; then printf '%s\n' 'Usage: phase3-gate.sh   (no arguments; the --independent partial mode is gone)'; exit 2; fi

leaks() { pgrep -af 'kwin_wayland --virtual|hi-kw-|hi-gui-|eis_pointer.py|eis_input.py|eis_input_wheel.py|kwin_burst.py|Xvfb -screen' | grep -v pgrep | tr '\n' ';' || true; }
scratch() { ls -d "${TMPDIR:-/tmp}"/hi-kwin-* "${TMPDIR:-/tmp}"/hi-xvfb-* "${TMPDIR:-/tmp}"/hi-gui-* 2>/dev/null | tr '\n' ' ' || true; }

python3 scripts/phase3-audit.py > "$EVID/before.json"
BEFORE_LEAKS="$(leaks)"; BEFORE_SCRATCH="$(scratch)"
echo "== leaks before: ${BEFORE_LEAKS:-none}; scratch before: ${BEFORE_SCRATCH:-none}"

if [[ "${SKIP_EARLIER:-0}" != 1 ]]; then
  for g in phase1 phase1_5; do
    echo "== ${g} gate (unchanged script)"
    "scripts/${g}-gate.sh" > "$EVID/gate-${g}.log" 2>&1 || { tail -n 30 "$EVID/gate-${g}.log"; echo "FAIL: ${g} gate"; exit 1; }
    grep -E '^PASS' "$EVID/gate-${g}.log" | tail -n 1
  done
fi
echo "== Phase 2.5 gate (runs the Phase 2 gate: fmt, clippy, all tests, bindings, npm audit, patch equality)"
scripts/phase2_5-gate.sh > "$EVID/gate-phase2_5.log" 2>&1 || { tail -n 40 "$EVID/gate-phase2_5.log"; echo "FAIL: Phase 2.5 gate"; exit 1; }
grep -E '^PASS|renderer.jsx ==' "$EVID/gate-phase2_5.log" | tail -n 3
grep -q '^PASS: Phase 2 gate' "$EVID/gate-phase2_5.log" || { echo "FAIL: nested Phase 2 gate did not pass"; exit 1; }

cd src-tauri
echo "== Phase 3 library tests by name"
cargo test --locked --lib 2>&1 | grep -E '^test |test result' > "$EVID/lib-tests.txt"
! grep -q FAILED "$EVID/lib-tests.txt"
for t in \
  state::machine::tests::spurious_leave_from_a_source_that_never_entered_is_a_no_op \
  state::machine::tests::current_generation_timer_before_its_deadline_is_ignored \
  state::machine::tests::hover_delay_zero_disables_and_pointer_panel_causes_are_distinct \
  state::actor::tests::actor_serializes_typed_view_changes_snapshots_and_injected_timer_ticks \
  window::input_region::tests::several_reports_per_rev_reanimate_layout_only_changes \
  window::input_region::tests::stale_revisions_and_late_unsettled_frames_cannot_reopen_a_collapsed_region \
  integrations::plasma::panel_socket::tests::client_beyond_capacity_waits_without_spinning_until_a_slot_frees \
  integrations::plasma::panel_socket::tests::panel_protocol_is_bounded_and_requires_a_complete_command \
  integrations::plasma::panel_socket::tests::native_panel_roundtrip_emits_only_allowed_events \
  integrations::plasma::anchor::tests::anchor_prefers_native_widget_on_primary_top_panel \
  integrations::plasma::status_file::tests::status_is_a_regular_private_file_updated_by_same_directory_rename \
  integrations::plasma::status_file::tests::a_reader_never_observes_a_partial_snapshot \
  integrations::plasma::status_file::tests::atomic_status_updates_preserve_foreign_replacements_even_with_same_pid \
  integrations::plasma::status_file::tests::in_place_overwrite_of_our_inode_is_also_detected_and_preserved \
  singleton::tests::stale_status_and_refusing_socket_are_reclaimed_then_ownership_is_ours \
  singleton::tests::reclaim_refuses_a_live_status_owner_even_with_a_stale_socket \
  singleton::tests::reclaim_refuses_a_live_socket_even_with_a_dead_status_pid \
  singleton::tests::reclaim_refuses_non_socket_symlinked_and_missing_sockets \
  singleton::tests::reclaim_refuses_unverifiable_or_non_regular_status \
  singleton::tests::reclaim_refuses_foreign_owned_artifacts \
  singleton::tests::instance_lock_excludes_second_owner_without_unlinking_socket \
  singleton::tests::live_socket_without_our_lock_is_never_unlinked \
  singleton::tests::live_legacy_status_without_socket_blocks_rust_and_preserves_status \
  singleton::tests::cleanup_never_removes_another_instances_status; do
  grep -qF "test $t ... ok" "$EVID/lib-tests.txt" || { echo "FAIL: required test missing or not ok: $t"; exit 1; }
done
echo "required library tests: all present and ok"

cd "$ROOT"
echo "== node store tests (events are the only writer; rev guard; ordered dispatch)"
node --test scripts/island-state.test.mjs 2>&1 | tee "$EVID/node-store-tests.txt" | grep -E '^ℹ (tests|pass|fail) '
grep -q '^ℹ fail 0' "$EVID/node-store-tests.txt"
grep -q '^ℹ pass [1-9]' "$EVID/node-store-tests.txt"

echo "== renderer: patch equality, no renderer-side state machine, hover timers or anchor mutation"
if ! (diff -u --label island/renderer.jsx --label island-tauri/ui/legacy/renderer.jsx "$REPO/island/renderer.jsx" ui/legacy/renderer.jsx || true) | cmp - ui/legacy/renderer.ownership.patch; then
  echo "FAIL: renderer.jsx drifted from island/renderer.jsx + renderer.ownership.patch"; exit 1
fi
if grep -nE 'useReducer|stateMachine|hoverTimer|leaveTimer|cancelHoverTimers|modeRef|onAnchor|setView\(|setStarted|api\.pin' ui/legacy/renderer.jsx; then echo "FAIL: renderer still owns state"; exit 1; fi
for f in monitor.jsx ai-stats.jsx state.cjs palette.cjs click-sound.cjs style.css; do cmp "$REPO/island/$f" "ui/legacy/$f"; done
echo "renderer.jsx = island/renderer.jsx + pinned patch; no leftover state/timers; other copies byte-identical"

cd src-tauri
echo "== nested-KWin Phase 3 scenarios (private KWin/bus/HOME; real EIS pointer+keyboard+wheel; real anchor-client.py)"
for t in kwin_phase3_host kwin_phase3_actor; do
  cargo test --locked --test "$t" -- --nocapture --test-threads=1 2>&1 | grep -E '^test |test result|panicked|timed out after .* waiting for:' | cut -c1-240 | tee "$EVID/$t.txt"
  grep -q 'test result: ok. 1 passed' "$EVID/$t.txt" || { echo "FAIL: $t"; exit 1; }
done

cd "$ROOT"
echo "== settled screenshots vs the Phase 2 baseline"
python3 scripts/phase3-screens.py

echo "== mutation evidence must be fresh and complete"
python3 scripts/phase3-mutations-check.py

python3 scripts/phase3-audit.py "$EVID/before.json"
AFTER_LEAKS="$(leaks)"; AFTER_SCRATCH="$(scratch)"
[[ "$BEFORE_LEAKS" == "$AFTER_LEAKS" ]] || { echo "FAIL: leaked processes: $AFTER_LEAKS"; exit 1; }
[[ "$BEFORE_SCRATCH" == "$AFTER_SCRATCH" ]] || { echo "FAIL: scratch dirs leaked: $AFTER_SCRATCH"; exit 1; }
echo "no leaked processes or scratch dirs (before == after)"
echo "PASS: Phase 3 gate (actor authority, ownership seam, hover sources, reclaim/refusals, rename status, rev-checked geometry; live island untouched)"
