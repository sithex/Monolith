"""Run every habby-metrics test with the Hermes python (no pytest needed):

    ~/.hermes/tools/python-<ver>/bin/python3 -I tests/run_tests.py

Unit tests (test_*.py, test_* functions) run in this process against a scratch HERMES_HOME; then the
integration harness runs twice in subprocesses (enabled / not-enabled). Results ->
<scratch>/metrics-plugin/ev/unit-results.json and harness-*.json.
"""
from __future__ import annotations

import importlib
import json
import subprocess
import sys
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from _common import EV, SCRATCH, isolate_env  # noqa: E402

isolate_env(SCRATCH / "unit-home")
results, failed = [], 0
only = sys.argv[1:]
for f in sorted(HERE.glob("test_*.py")):
    m = importlib.import_module(f.stem)
    for name in sorted(n for n in dir(m) if n.startswith("test_")):
        if only and not any(o in name for o in only):
            continue
        t = time.perf_counter()
        try:
            getattr(m, name)()
            ok, err = True, None
        except Exception:
            ok, err = False, traceback.format_exc()
            failed += 1
        ms = round((time.perf_counter() - t) * 1000, 1)
        results.append({"test": f"{f.stem}.{name}", "ok": ok, "ms": ms, "error": err})
        print(("PASS " if ok else "FAIL ") + f"{f.stem}.{name} ({ms} ms)")
        if err:
            print(err)
EV.mkdir(parents=True, exist_ok=True)
(EV / "unit-results.json").write_text(json.dumps({"python": sys.version.split()[0], "results": results}, indent=1),
                                      encoding="utf-8")
print(f"unit: {len(results) - failed}/{len(results)} passed")
if not only:
    for mode in ("not-enabled", "enabled"):
        print(f"== harness {mode}")
        r = subprocess.run([sys.executable, "-I", str(HERE / "harness_hermes.py"), mode], text=True,
                           capture_output=True, timeout=600)
        print("\n".join(l for l in (r.stdout + r.stderr).splitlines() if l.startswith(("PASS", "FAIL", "{", "hook", "Traceback", "  "))
                        or "Error" in l)[-6000:])
        if r.returncode != 0:
            failed += 1
            print(f"harness {mode}: exit {r.returncode}")
sys.exit(1 if failed else 0)
