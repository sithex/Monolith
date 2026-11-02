#!/usr/bin/env python3
"""Phase 3 mutation belt: the six kept mutations + the controller-audit pair + five post-review
mutations. Only owned sources below island-tauri are changed, one COMPILING (or buildable)
mutation at a time. Every mutation is restored in `finally` and diff-checked against a scratch
copy; UI mutations rebuild `dist/` before and after so the embedded assets match the source.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "src-tauri"
EVID = ROOT / "evidence/phase3/mutations"
EVID.mkdir(parents=True, exist_ok=True)
MACHINE = "src/state/machine.rs"
RENDERER = "../ui/legacy/renderer.jsx"
ACTOR_GUI = ["--test", "kwin_phase3_actor", "actor_owns_hover_tabs_focus_cues_and_geometry"]
ACTOR_GUI_NAME = "actor_owns_hover_tabs_focus_cues_and_geometry"
HOST_GUI = ["--test", "kwin_phase3_host", "native_anchor_client_pin_coexistence_and_sigterm_cleanup"]
HOST_GUI_NAME = "native_anchor_client_pin_coexistence_and_sigterm_cleanup"

# (label, file relative to src-tauri, [(old, new), ...], cargo args, expected failing test name)
CASES = [
    # ---- the six kept from the first Phase 3 run (unchanged) ----
    ("actor-select-view-ignored", MACHINE, [("self.state.view = view;", "let _ignored_view = view;")], ["--lib", "state::actor::tests::actor_serializes_typed_view_changes_snapshots_and_injected_timer_ticks"], "actor_serializes_typed_view_changes_snapshots_and_injected_timer_ticks"),
    ("foreign-socket-unlinked", "src/singleton.rs", [('anyhow::bail!("Existing panel socket without a status; refusing takeover")', '{ fs::remove_file(&socket_path)?; return Ok(()); }')], ["--lib", "singleton::tests::live_socket_without_our_lock_is_never_unlinked"], "live_socket_without_our_lock_is_never_unlinked"),
    ("incomplete-command-accepted", "src/integrations/plasma/panel_socket.rs", [('b"pin\\n" => Some(PanelCommand::Pin),', 'b"pin\\n" | b"pin" => Some(PanelCommand::Pin),')], ["--lib", "panel_protocol_is_bounded_and_requires_a_complete_command"], "panel_protocol_is_bounded_and_requires_a_complete_command"),
    ("activity-priority-inverted", MACHINE, [("b.priority\n                .cmp(&a.priority)", "a.priority\n                .cmp(&b.priority)")], ["--lib", "activity_priority_ties_expiry_split_merge_and_availability"], "activity_priority_ties_expiry_split_merge_and_availability"),
    ("notification-reverted-to-toolbar", "src/window/mod.rs", [("spike.map_or(gdk::WindowTypeHint::Notification,", "spike.map_or(gdk::WindowTypeHint::Toolbar,")], HOST_GUI, HOST_GUI_NAME),
    ("zero-hover-enabled", MACHINE, [("if settings.hover_delay > 0.", "if settings.hover_delay >= 0.")], ["--lib", "hover_delay_zero_disables_and_pointer_panel_causes_are_distinct"], "hover_delay_zero_disables_and_pointer_panel_causes_are_distinct"),
    # ---- controller audit findings (a) and (b) ----
    ("timer-fired-before-deadline-accepted", MACHINE, [(".is_some_and(|d| d.generation == generation && d.at_ms <= now)", ".is_some_and(|d| d.generation == generation)")], ["--lib", "current_generation_timer_before_its_deadline_is_ignored"], "current_generation_timer_before_its_deadline_is_ignored"),
    ("panel-socket-capacity-off-by-one", "src/integrations/plasma/panel_socket.rs", [("events: if clients.len() < MAX_CLIENTS {", "events: if clients.len() <= MAX_CLIENTS {")], ["--lib", "client_beyond_capacity_waits_without_spinning_until_a_slot_frees"], "client_beyond_capacity_waits_without_spinning_until_a_slot_frees"),
    # ---- five added after the Phase 3 review ----
    ("renderer-tab-click-uses-local-view", RENDERER, [
        ('onClick={() => { setDirection(viewIds.indexOf(id) > viewIds.indexOf(view) ? 1 : -1); dispatchIntent({ type: "selectView", view: id }).catch(fail); }}',
         'onClick={() => { setDirection(viewIds.indexOf(id) > viewIds.indexOf(view) ? 1 : -1); setLocalView(id); }}'),
        ("  const [direction, setDirection] = useState(1);\n", "  const [direction, setDirection] = useState(1);\n  const [, setLocalView] = useState(null);\n"),
    ], ACTOR_GUI, ACTOR_GUI_NAME),
    ("renderer-leave-timer-reintroduced", RENDERER, [
        ('const leave = () => { dispatchIntent({ type: "pointerLeave" }).catch(fail); };',
         'const leave = () => { dispatchIntent({ type: "pointerLeave" }).catch(fail); setTimeout(() => dispatchIntent({ type: "collapse" }).catch(fail), 300); };'),
    ], ACTOR_GUI, ACTOR_GUI_NAME),
    ("stale-or-future-rev-geometry-accepted", "src/commands/island.rs", [("if actor_geometry && (report.state_rev != current.rev || report.mode != current.mode) {", "if actor_geometry && report.mode != current.mode {")], ACTOR_GUI, ACTOR_GUI_NAME),
    ("spurious-leave-schedules-grace", MACHINE, [("if was_inside && !self.pointer_inside && !self.panel_inside {", "if !self.pointer_inside && !self.panel_inside {")], ["--lib", "state::machine::tests::spurious_leave_from_a_source_that_never_entered_is_a_no_op"], "spurious_leave_from_a_source_that_never_entered_is_a_no_op"),
    ("reclaim-when-socket-accepts", "src/singleton.rs", [('Probe::Live => anyhow::bail!("Existing panel socket is live; refusing takeover"),', "Probe::Live => {}")], ["--lib", "singleton::tests::reclaim_refuses_a_live_socket_even_with_a_dead_status_pid"], "reclaim_refuses_a_live_socket_even_with_a_dead_status_pid"),
]


def build_ui(log):
    with log.open("a") as output:
        subprocess.run(["nice", "-n", "10", "npm", "run", "build:ui"], cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, check=True, timeout=300)


def run(args, log):
    # No --nocapture: a failing test prints "test NAME ... FAILED" as one intact line.
    with log.open("a") as output:
        result = subprocess.run(["nice", "-n", "10", "cargo", "test", "--locked", *args], cwd=CRATE, stdout=output, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, log.read_text()


def scratch_name(name):
    return name.replace("../", "up/")


def main():
    records = []
    with tempfile.TemporaryDirectory(prefix="phase3-mutations-", dir=os.environ["TMPDIR"]) as directory:
        pristine = Path(directory)
        files = {}
        for _, name, *_ in CASES:
            if name not in files:
                files[name] = (CRATE / name).read_bytes()
                target = pristine / scratch_name(name)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(files[name])
        try:
            for label, name, edits, args, expected in CASES:
                path = CRATE / name
                text = files[name].decode()
                for old, _ in edits:
                    if text.count(old) != 1:
                        raise RuntimeError(f"mutation anchor not unique: {label}: {old[:60]!r}")
                for old, new in edits:
                    text = text.replace(old, new, 1)
                log = EVID / f"{label}.log"
                log.write_text("")
                ui = name.startswith("../ui/")
                try:
                    path.write_text(text)
                    if ui:
                        build_ui(log)
                    code, output = run(args, log)
                    if code == 0 or f"{expected} ... FAILED" not in output or "could not compile" in output:
                        raise RuntimeError(f"mutation did not fail the named executable test: {label}")
                    failures = [line for line in output.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
                    panics = [line.strip() for line in output.splitlines() if "panicked at" in line or "timed out after" in line or line.strip().startswith(("left:", "right:"))]
                    record = {"mutation": label, "failedTests": failures, "firstFailure": panics[:3], "restored": True}
                    records.append(record)
                    print(json.dumps(record), flush=True)
                finally:
                    path.write_bytes(files[name])
                    os.utime(path, None)  # invalidate Cargo's mutated fingerprint
                    if ui:
                        build_ui(log)
                    diff = subprocess.run(["diff", "-u", str(pristine / scratch_name(name)), str(path)], capture_output=True, text=True)
                    (EVID / f"{label}.diff.log").write_text(diff.stdout + diff.stderr)
                    if diff.returncode != 0:
                        raise RuntimeError(f"restore diff failed: {label}")
        finally:
            for name, data in files.items():
                (CRATE / name).write_bytes(data)
                os.utime(CRATE / name, None)
        (EVID / "restored-lib.log").write_text("")
        code, output = run(["--lib"], EVID / "restored-lib.log")
        if code != 0:
            raise RuntimeError("restored product library tests failed")
        (EVID / "restored-gui.log").write_text("")
        code, output = run(["--test", "kwin_phase3_actor", "--test", "kwin_phase3_host", "--", "--test-threads=1"], EVID / "restored-gui.log")
        if code != 0:
            raise RuntimeError("restored Phase 3 GUI tests failed")
        if any((CRATE / name).read_bytes() != data for name, data in files.items()):
            raise RuntimeError("final pristine diff failed")
    (EVID / "summary.json").write_text(json.dumps(records, indent=2) + "\n")
    print(f"PASS: {len(records)} compiling/buildable mutations each failed a named test; restored sources diff-identical; restored lib + GUI tests green")


if __name__ == "__main__":
    main()
