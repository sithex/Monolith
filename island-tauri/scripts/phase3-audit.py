#!/usr/bin/env python3
"""Read-only Phase 3 safety snapshot; never open the live data directory or bus."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
SKIP = {"node_modules", "dist", "target", "__pycache__", "evidence", ".git"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree(paths):
    result = {}
    for path in paths:
        candidates = [path] if path.is_file() else path.rglob("*")
        for candidate in candidates:
            relative = candidate.relative_to(REPO)
            if not any(part in SKIP for part in relative.parts) and candidate.is_file():
                result[str(relative)] = digest(candidate)
    return result


LIVE_EXE = str(REPO / "island-rust/target/release/hermes-island")


def live():
    # Controller amendment (after Phase 3b review): discover the live island by /proc/PID/exe
    # instead of a hard-coded PID, so the audit survives a reboot. Absent live island -> [].
    found = []
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            if os.readlink(proc / "exe") != LIVE_EXE:
                continue
            stat = (proc / "stat").read_text().rsplit(")", 1)[1].split()
        except OSError:
            continue
        found.append({"pid": int(proc.name), "exe": LIVE_EXE, "startTicks": stat[19]})
    return sorted(found, key=lambda entry: entry["pid"])


def snapshot():
    return {
        "live": live(),
        "protected": tree([REPO / name for name in ["island", "island-rust/src", "island-rust/Cargo.toml", "island-rust/Cargo.lock", "island-native/src", "src", "package.json", ".gitignore"]]),
        "legacy": tree([ROOT / "ui/legacy"]),
        "earlierGates": tree([ROOT / "scripts" / f"{phase}-gate.sh" for phase in ["phase1", "phase1_5", "phase2", "phase2_5"]]),
        "earlierTests": tree([ROOT / "src-tauri/tests", ROOT / "scripts/phase2-mutations.sh", ROOT / "scripts/phase2_5_mutations.py"]),
        "xSockets": sorted(p.name for p in Path("/tmp/.X11-unix").iterdir()),
        "gitOutside": [line for line in subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO, text=True).splitlines() if " island-tauri/" not in line],
    }


if __name__ == "__main__":
    current = snapshot()
    if len(sys.argv) == 1:
        print(json.dumps(current, indent=2, sort_keys=True))
    else:
        before = json.loads(Path(sys.argv[1]).read_text())
        for key in ["live", "protected", "legacy", "earlierGates", "xSockets", "gitOutside"]:
            if before[key] != current[key]:
                raise SystemExit(f"FAIL: Phase 3 safety snapshot changed: {key}")
        for name, expected in before["earlierTests"].items():
            path = REPO / name
            if current["earlierTests"].get(name) == expected:
                continue
            if name == "island-tauri/src-tauri/tests/common/mod.rs":
                original = path.read_text().replace(
                    'let notification = atom(conn, identity::WINDOW_TYPE_ATOM);',
                    'let toolbar = atom(conn, "_NET_WM_WINDOW_TYPE_TOOLBAR");',
                ).replace(' == notification),', ' == toolbar),').replace(
                    '"window type must be NOTIFICATION"', '"window type must be TOOLBAR"'
                )
                if hashlib.sha256(original.encode()).hexdigest() == expected:
                    continue
            raise SystemExit(f"FAIL: unauthorized earlier-test edit: {name}")
        print("PASS: live PID/exe/start, protected sources, legacy copies, earlier gates, X sockets and outside git state unchanged")
