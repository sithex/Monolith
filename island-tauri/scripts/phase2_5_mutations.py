#!/usr/bin/env python3
"""Phase 2.5 mutation belt for tests/kwin_spike_panel.rs.

Each mutation breaks ONE behaviour (product or page), runs the spike scenario, requires it to FAIL
with the expected assertion text (a compile error or an unrelated failure does not count), restores
the pristine bytes (mtime refreshed so cargo/vite rebuild) and `diff`s them against a snapshot.
Usage: scripts/phase2_5_mutations.py [name ...]   (default: all)   from island-tauri/.
"""
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "src-tauri"
LOGS = ROOT / "evidence" / "phase2_5" / "mutations"
SPIKE_TS = "ui/spike-main.tsx"
WINDOW_RS = "src-tauri/src/window/mod.rs"
LINUX_RS = "src-tauri/src/window/input_region/linux.rs"

# name, file, old, new, extra env, substring that must appear in the failure output
MUTATIONS = [
    ("region-covers-pill-readback", SPIKE_TS,
     'void reportGeometry(report(rev, 0, true, "still"))',
     'void reportGeometry(report(rev, 0, true, "hover"))',
     {}, "empty input region over the pill"),
    ("region-covers-pill-physical", SPIKE_TS,
     'void reportGeometry(report(rev, 0, true, "still"))',
     'void reportGeometry(report(rev, 0, true, "hover"))',
     {"SPIKE_MUTATION_SKIP_REGION_READBACK": "1"}, "panel pill got no hover (enter) at the pill"),
    ("island-loses-keep-above", WINDOW_RS,
     "gtk_window.set_keep_above(spike.is_none_or(|s| s.above));",
     "gtk_window.set_keep_above(false);",
     {}, "stacking list and composited pixel at the pill must agree that the island is over the panel"),
    ("notification-hint-becomes-toolbar", WINDOW_RS,
     "spike::SpikeWindowType::Notification => gdk::WindowTypeHint::Notification,",
     "spike::SpikeWindowType::Notification => gdk::WindowTypeHint::Toolbar,",
     {}, "window type must be _NET_WM_WINDOW_TYPE_NOTIFICATION"),
    ("focus-on-map", WINDOW_RS,
     "gtk_window.set_focus_on_map(false);",
     "gtk_window.set_focus_on_map(true);",
     {}, "island must request no focus on map"),
    ("blur-region-never-set", LINUX_RS,
     "        if self.blur {",
     "        if false && self.blur {",
     {}, "blur property must be set when open"),
    ("empty-blur-region-is-an-empty-property", LINUX_RS,
     "    if rows.is_empty() {\n        conn.delete_property",
     "    if false {\n        conn.delete_property",
     {}, "empty Still region"),
    ("morph-starts-6px-off-the-pill", SPIKE_TS,
     "const PILL: Rect = { x: 515,",
     "const PILL: Rect = { x: 521,",
     {}, "is not a coherent island"),
    ("torn-frames-height-lags-width", SPIKE_TS,
     "island.style.height = `${r.h}px`;",
     "island.style.height = `${rectAt(t * 0.6).h}px`;",
     {}, "is not a coherent island"),
]


def run(cmd, cwd, env=None, timeout=900):
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)


def build_ui():
    r = run(["npm", "run", "build:ui"], ROOT)
    if r.returncode:
        sys.exit("ui build failed:\n" + r.stdout[-2000:] + r.stderr[-2000:])
    (TAURI / "src" / "lib.rs").touch()  # tauri embeds dist/: force the re-embed


def main():
    wanted = sys.argv[1:] or [m[0] for m in MUTATIONS]
    LOGS.mkdir(parents=True, exist_ok=True)
    results = []
    for name, rel, old, new, extra, expect in MUTATIONS:
        if name not in wanted:
            continue
        path = ROOT / rel
        pristine = path.read_text()
        snapshot = LOGS / f"{name}.pristine"
        snapshot.write_text(pristine)
        if pristine.count(old) != 1:
            sys.exit(f"{name}: expected exactly one occurrence of the mutation target in {rel}, found {pristine.count(old)}")
        status = "?"
        try:
            path.write_text(pristine.replace(old, new))
            if rel.endswith(".tsx"):
                build_ui()
            env = dict(os.environ, **extra)
            r = run(["nice", "-n", "10", "cargo", "test", "--locked", "--test", "kwin_spike_panel",
                     "kwin_spike_panel", "--", "--exact", "kwin_spike_panel", "--nocapture"], TAURI, env)
            out = r.stdout + r.stderr
            (LOGS / f"{name}.log").write_text(out)
            if "error[E" in out or "could not compile" in out:
                status = "INFRA: did not compile (proves nothing)"
            elif r.returncode == 0:
                status = "SURVIVED (test did not notice)"
            elif expect in out:
                line = next((l for l in out.splitlines() if expect in l), expect)
                status = "KILLED: " + line.strip()[:200]
            else:
                tail = [l for l in out.splitlines() if "panicked" in l or "timed out" in l][:2]
                status = "FAILED BUT NOT WITH THE EXPECTED TEXT: " + " | ".join(tail)[:300]
        finally:
            path.write_text(pristine)
            os.utime(path, None)
            if rel.endswith(".tsx"):
                build_ui()
            same = subprocess.run(["diff", "-u", str(snapshot), str(path)], capture_output=True, text=True)
            restored = "identical" if same.returncode == 0 else "DIFFERS"
        results.append((name, status, restored))
        print(f"{name}: {status} | restore {restored}", flush=True)
    bad = [r for r in results if not r[1].startswith("KILLED") or r[2] != "identical"]
    print("RESULT:", "ALL KILLED AND RESTORED" if not bad else f"PROBLEMS: {bad}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
