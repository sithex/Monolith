"""Check that habby-metrics loads from an installed (scratch) home via Hermes's real discover_plugins().

    <hermes python> -I tests/verify_home.py <HERMES_HOME> [--allow-live]

Read-only: runs Hermes's plugin discovery (loading the plugin only registers hooks; no hook fires, no thread
starts, nothing is written). Refuses the live home unless --allow-live (install.sh passes it to verify a real
install). Prints one JSON line; exit 0 when the plugin is listed AND enabled."""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _common import isolate_env  # noqa: E402

home = isolate_env(Path(sys.argv[1]), allow_live="--allow-live" in sys.argv[2:])
from hermes_cli import plugins as P  # noqa: E402

P.discover_plugins()
pm = P.get_plugin_manager()
info = {k: {"enabled": v.enabled, "error": v.error, "source": v.manifest.source} for k, v in pm._plugins.items()
        if "habby" in k}
hooks = sorted(h for h, cbs in pm._hooks.items() if any("habby" in getattr(cb, "__module__", "") for cb in cbs))
print(json.dumps({"home": str(home), "plugins": info, "hooks": hooks}))
sys.exit(0 if info.get("habby-metrics", {}).get("enabled") else 1)
