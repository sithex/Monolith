#!/usr/bin/env python3
"""Read-only /proc sampler: CPU % and wakeups/s of named processes over N-second windows.

  sample_proc.py [--secs 60] [--windows 1] [--names plasmashell,kwin_wayland]

Prints one JSON line per window (cpu_pct, wakeups_s, busiest threads). Never signals or
attaches to anything; it only reads /proc/<pid>/{stat,status,task/*}.
"""
import argparse, json, os, subprocess, time

HZ = os.sysconf("SC_CLK_TCK")
TOP = 4


def pid_of(name):
    out = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True).stdout.split()
    return int(out[0]) if out else None


def snap(pid):
    s = open(f"/proc/{pid}/stat").read()
    f = s[s.rfind(")") + 2:].split()
    threads = {}
    for t in os.listdir(f"/proc/{pid}/task"):
        try:
            name = open(f"/proc/{pid}/task/{t}/comm").read().strip()
            sw = 0
            for line in open(f"/proc/{pid}/task/{t}/status"):
                if "ctxt_switches" in line:
                    sw += int(line.split()[1])
            ts = open(f"/proc/{pid}/task/{t}/stat").read()
            tf = ts[ts.rfind(")") + 2:].split()
            threads[t] = (name, sw, int(tf[11]) + int(tf[12]))
        except OSError:
            pass
    return int(f[11]) + int(f[12]), threads, int(f[13]) + int(f[14])


def window(pids, secs):
    a = {n: snap(p) for n, p in pids.items()}
    t0 = time.monotonic()
    time.sleep(secs)
    b = {n: snap(p) for n, p in pids.items()}
    dt = time.monotonic() - t0
    out = {}
    for n in pids:
        cpu = (b[n][0] - a[n][0]) / HZ / dt * 100
        rows = []
        for k, v in b[n][1].items():
            if k in a[n][1]:
                rows.append(((v[1] - a[n][1][k][1]) / dt, v[0], (v[2] - a[n][1][k][2]) / HZ / dt * 100))
        wake = sum(r[0] for r in rows)
        rows.sort(reverse=True)
        kids = (b[n][2] - a[n][2]) / HZ / dt * 100  # reaped children (spawned helpers)
        out[n] = {"cpu_pct": round(cpu, 2), "children_cpu_pct": round(kids, 2), "wakeups_s": round(wake, 1),
                  "top": [[round(w, 1), name, round(c, 2)] for w, name, c in rows[:TOP]]}
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--secs", type=float, default=60)
    ap.add_argument("--windows", type=int, default=1)
    ap.add_argument("--names", default="plasmashell,kwin_wayland")
    ap.add_argument("--top", type=int, default=4)
    o = ap.parse_args()
    global TOP
    TOP = o.top
    pids = {n: pid_of(n) for n in o.names.split(",")}
    pids = {n: p for n, p in pids.items() if p}
    for _ in range(o.windows):
        print(json.dumps({"t": time.strftime("%H:%M:%S"), "load": os.getloadavg()[0], **window(pids, o.secs)}), flush=True)


if __name__ == "__main__":
    main()
