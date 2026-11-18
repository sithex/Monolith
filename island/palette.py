"""Read-only adapter to the installed Hermes skin engine; no CLI startup hooks."""
import json
import re
import sys

sys.path.insert(0, sys.argv[1])
# Resolve the committed dependency environment without bootstrap/repair/PM writes.
from pathlib import Path
from pm.environments import selected_venv, site_packages
sys.path.insert(1, str(site_packages(selected_venv(Path(sys.argv[1])))))
from hermes_cli.config import load_config_readonly
from hermes_cli.skin_engine import init_skin_from_config, get_active_skin

config = load_config_readonly() or {}
# Monolith's optional theme sync activates its own generated `monolith` skin (migrated from
# the older `ripple-island` name). The island's "Hermes skin" scheme must keep following the
# skin the user had BEFORE that, which the generated file remembers as `monolith_base` (older
# files used `ripple_island_base`); otherwise picking any island theme would make "Hermes
# skin" mirror the island's own colours and lose the user's real skin.
# Only while theme sync is ON (the island passes --theme-sync): with sync off, an active
# `monolith` (or the older `ripple-island`) skin is one the user picked (e.g. carried over
# from another machine), and swapping it for its remembered base showed a different skin's
# colours (always necron green).
OUR_GENERATED = frozenset(("monolith", "ripple-island"))
BASE_KEYS = ("monolith_base", "ripple_island_base")


def _remembered_base(home: Path) -> str:
    """The base skin remembered by our generated skin under either name/key."""
    for skin in ("monolith", "ripple-island"):
        try:
            data = json.loads((home / "skins" / f"{skin}.yaml").read_text(encoding="utf-8-sig"))
        except (OSError, ValueError, AttributeError):
            continue
        if not isinstance(data, dict):
            continue
        for key in BASE_KEYS:
            value = data.get(key)
            if isinstance(value, str) and value.strip() and value not in OUR_GENERATED:
                return value.strip()
    return ""


display = config.get("display") if isinstance(config, dict) else None
if "--theme-sync" in sys.argv[2:] and isinstance(display, dict) and display.get("skin") in OUR_GENERATED:
    try:
        from hermes_constants import get_hermes_home
        home = Path(get_hermes_home())
    except Exception:
        import os
        home = Path(os.environ.get("HERMES_HOME") or Path.home() / ".hermes")
    base = _remembered_base(home) or "default"
    config = {**config, "display": {**display, "skin": base}}
init_skin_from_config(config)
skin = get_active_skin()
roles = {
    "background": ("background", "#111111"),
    "accent": ("ui_accent", "#dddddd"),
    "text": ("ui_text", "#dddddd"),
    "primary": ("ui_primary", "#eeeeee"),
    "dim": ("banner_dim", "#999999"),
    "border": ("ui_border", "#444444"),
    "surface": ("status_bar_bg", "#222222"),
    "error": ("ui_error", "#ff8888"),
}
palette = {"name": str(skin.name)[:80]}
for role, (key, fallback) in roles.items():
    value = skin.get_color(key, fallback)
    palette[role] = value.lower() if isinstance(value, str) and re.fullmatch(r"#[0-9a-fA-F]{6}", value) else fallback
print(json.dumps(palette))
