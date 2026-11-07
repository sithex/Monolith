#!/usr/bin/env python3
"""A/B runner for the notch idle harness (NotchIdleHarness.qml).

Runs the harness under QT_QPA_PLATFORM=offscreen for each (version, scenario) and reads
CPU (utime+stime), wakeups (voluntary+involuntary context switches over all threads) and
frames swapped per window, over the counted window only.

  run_idle_ab.py --ui NAME=DIR [--ui NAME=DIR ...] [--secs 30] [--repeat 2] [--only S1,S2]

DIR is a plasma-anchor `contents/ui` directory (e.g. a `git archive` of an older commit).
Prints one line per run and a median table; --json writes all runs.
"""
import argparse, json, os, statistics, subprocess, sys, time

HZ = os.sysconf("SC_CLK_TCK")
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "qmlrenders"))
import qt6_tools  # noqa: E402  (Qt 6 tool lookup shared with qmlrenders)

SCENARIOS = {
    # Pointer parked on DP-1 (warm 70 s so the 1-minute "settled" state is reached).
    "parked-settled": {"pointer": "parked", "warmSecs": 70},
    # Pointer parked a few seconds ago (the common moment right after you stop moving it).
    "parked-fresh": {"pointer": "parked"},
    "parked-playing": {"pointer": "parked", "playing": True},
    "parked-playing-settled": {"pointer": "parked", "playing": True, "warmSecs": 70},
    "moving": {"pointer": "moving"},
    "moving-playing": {"pointer": "moving", "playing": True},
    "working-parked": {"pointer": "parked", "working": True},
    "waiting-parked": {"pointer": "parked", "waiting": True},
    "companion-off-playing": {"pointer": "parked", "playing": True, "companion": False},
    "companion-off-idle": {"pointer": "none", "companion": False},
    "no-pointer-yet": {"pointer": "none"},
    "frozen-moving-playing": {"pointer": "moving", "playing": True, "working": True, "frozen": True},
    "frozen-parked-waiting": {"pointer": "parked", "waiting": True, "frozen": True},
}
for ch in ("blob", "notchmimic", "companioncube", "nurgling", "hollowknight", "frog"):
    SCENARIOS[f"{ch}-parked"] = {"pointer": "parked", "character": ch}
    SCENARIOS[f"{ch}-moving"] = {"pointer": "moving", "character": ch}


def proc_sample(pid):
    s = open(f"/proc/{pid}/stat").read()
    f = s[s.rfind(")") + 2:].split()
    cpu = int(f[11]) + int(f[12])
    wake = 0
    for t in os.listdir(f"/proc/{pid}/task"):
        try:
            for line in open(f"/proc/{pid}/task/{t}/status"):
                if "ctxt_switches" in line:
                    wake += int(line.split()[1])
        except OSError:
            pass
    return cpu, wake, time.monotonic()


def run(ui, scenario, secs):
    sc = dict(scenario, secs=secs)
    env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_FORCE_STDERR_LOGGING="1")
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("DISPLAY", None)
    url = "file://" + os.path.abspath(ui)
    cmd = [qt6_tools.require("qml"), os.path.join(HERE, "NotchIdleHarness.qml"), "--", url, json.dumps(sc)]
    p = subprocess.Popen(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    a = b = None
    frames = None
    errors = []
    for line in p.stdout:
        if "HARNESS-START" in line:
            a = proc_sample(p.pid)
        elif "HARNESS-RESULT" in line:
            b = proc_sample(p.pid)
            frames = json.loads(line.split("HARNESS-RESULT", 1)[1])["frames"]
        elif "rror" in line or "TypeError" in line or "ReferenceError" in line:
            errors.append(line.strip()[:200])
    p.wait()
    if not a or not b:
        raise RuntimeError(f"harness failed: {errors[:5]}")
    dt = b[2] - a[2]
    return {
        "cpu_pct": round((b[0] - a[0]) / HZ / dt * 100, 2),
        "wakeups_s": round((b[1] - a[1]) / dt, 1),
        "frames_s": [round(f / dt, 2) for f in frames],
        "frames_total_s": round(sum(frames) / dt, 2),
        "errors": errors[:3],
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ui", action="append", required=True)
    ap.add_argument("--secs", type=float, default=30)
    ap.add_argument("--repeat", type=int, default=2)
    ap.add_argument("--only", default="")
    ap.add_argument("--json", default="")
    o = ap.parse_args()
    names = [s for s in o.only.split(",") if s] or list(SCENARIOS)
    uis = [u.split("=", 1) for u in o.ui]
    runs = []
    for r in range(o.repeat):
        for name in names:
            for ver, d in uis:
                res = run(d, SCENARIOS[name], o.secs)
                res.update(version=ver, scenario=name, round=r)
                runs.append(res)
                print(json.dumps(res), flush=True)
    print("\nmedian  scenario                     version     cpu%   wake/s  frames/s [DP-2, DP-1, HDMI]")
    for name in names:
        for ver, _ in uis:
            rs = [x for x in runs if x["scenario"] == name and x["version"] == ver]
            med = lambda k: statistics.median(x[k] for x in rs)
            fr = [statistics.median(x["frames_s"][i] for x in rs) for i in range(3)]
            print(f"        {name:28s} {ver:10s} {med('cpu_pct'):6.2f} {med('wakeups_s'):8.1f}  {sum(fr):6.2f} {fr}")
    if o.json:
        json.dump(runs, open(o.json, "w"), indent=1)


if __name__ == "__main__":
    sys.exit(main())
