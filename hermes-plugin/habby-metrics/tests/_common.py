"""Shared setup for the habby-metrics tests: scratch HERMES_HOME, Hermes source on sys.path, a fresh
copy of the plugin module per call. Never touches the live Hermes home."""
from __future__ import annotations

import importlib.util
import os
import shutil
import sys
import tempfile
from pathlib import Path

PLUGIN_DIR = Path(__file__).resolve().parent.parent
# Read before isolate_env() strips HERMES_*: the live home is HERMES_HOME (or ~/.hermes). isolate_env() re-exports
# it as METRICS_TEST_LIVE_HOME so the harness subprocesses (whose HERMES_HOME is a scratch home) resolve the same one.
LIVE_HOME = Path(os.environ.get("METRICS_TEST_LIVE_HOME") or os.environ.get("HERMES_HOME")
                 or Path.home() / ".hermes").expanduser().resolve()
HERMES_SRC = Path(os.environ.get("HABBY_HERMES_SRC") or next(
    (p for p in (LIVE_HOME / "hermes-agent", Path.home() / ".hermes" / "hermes-agent") if (p / "hermes_bootstrap.py").exists()),
    LIVE_HOME / "hermes-agent"))
SCRATCH = Path(os.environ.get("HABBY_METRICS_SCRATCH") or LIVE_HOME / "cache" / "scratch" / "metrics-plugin")
EV = SCRATCH / "ev"


def isolate_env(home: Path, allow_live: bool = False) -> Path:
    """Strip inherited Hermes/Habby env (this may run inside a live Hermes) and point at a scratch home.
    allow_live: read-only plugin discovery against the live home (install.sh's verification only)."""
    for k in list(os.environ):
        if k.startswith(("HERMES_", "_HERMES", "TERMINAL_", "TABBY_", "HABBY_", "QA_")):
            del os.environ[k]
    home = Path(home).resolve()
    if home == LIVE_HOME and not allow_live:
        raise SystemExit("refusing to use the live Hermes home")
    home.mkdir(parents=True, exist_ok=True)
    os.environ["HERMES_HOME"] = str(home)
    os.environ["METRICS_TEST_LIVE_HOME"] = str(LIVE_HOME)
    os.environ["HABBY_METRICS_PLAN"] = "0"           # never the network from tests
    os.environ["HERMES_GATEWAY_LOCK_DIR"] = str(home / "locks")
    if str(HERMES_SRC) not in sys.path:
        sys.path.insert(0, str(HERMES_SRC))
    import hermes_bootstrap  # noqa: F401  (puts Hermes's dependency env on sys.path, as bin/hermes does)
    return home


_n = 0


def load_plugin(name: str = "habby_metrics_ut"):
    """A fresh, independent module instance of the plugin (own queue/state)."""
    global _n
    _n += 1
    spec = importlib.util.spec_from_file_location(f"{name}_{_n}", PLUGIN_DIR / "__init__.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def tmpdir(prefix: str) -> Path:
    base = SCRATCH / "tmp"
    base.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix=prefix, dir=base))


def rmtree(p: Path) -> None:
    shutil.rmtree(p, ignore_errors=True)


class Feed:
    """Routes the plugin's hooks into a synchronous Metrics instance (no threads)."""

    def __init__(self, mod, out: Path, **kw):
        self.mod = mod
        self.m = mod.Metrics(out, **kw)
        self.t = 1_800_000_000.0
        mod._put = self._put  # hooks -> here instead of the global queue

    def _put(self, kind, d):
        self.t += 0.5
        self.m.apply(kind, self.t, d)

    def hook(self, name, **kw):
        fn = dict(self.mod.HOOKS)[name]
        return fn(**kw)

    def flush(self):
        self.m.flush_events()
        self.m.write_snapshot(self.t)
