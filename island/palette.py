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
# Ripple's optional theme sync activates its own generated `ripple-island` skin. The island's
# "Hermes skin" scheme must keep following the skin the user had BEFORE that, which the
# generated file remembers as `ripple_island_base`; otherwise picking any island theme would
# make "Hermes skin" mirror the island's own colours and lose the user's real skin.
display = config.get("display") if isinstance(config, dict) else None
if isinstance(display, dict) and display.get("skin") == "ripple-island":
    try:
        from hermes_constants import get_hermes_home
        home = Path(get_hermes_home())
    except Exception:
        import os
        home = Path(os.environ.get("HERMES_HOME") or Path.home() / ".hermes")
    try:
        base = json.loads((home / "skins" / "ripple-island.yaml").read_text(encoding="utf-8-sig")).get("ripple_island_base")
    except (OSError, ValueError, AttributeError):
        base = None
    base = base if isinstance(base, str) and base.strip() and base != "ripple-island" else "default"
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
