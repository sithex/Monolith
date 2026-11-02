"""Mirror the Ripple island colour scheme into a Hermes skin, or restore the previous one.

Usage: hermes-theme.py <hermes-agent-dir> <hermes-launcher> apply '<palette json>'
       hermes-theme.py <hermes-agent-dir> <hermes-launcher> restore

Only public, documented Hermes surfaces are written: a skin file
``$HERMES_HOME/skins/ripple-island.yaml`` (JSON is valid YAML) and ``display.skin`` through
the ``hermes skin use`` CLI (falling back to ``hermes config set display.skin``). The skin
copies the previous skin's branding/spinner/art so only colours change, and remembers that
skin under ``ripple_island_base`` so turning sync off restores it. Prints one JSON line.
"""
import copy
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

SKIN = "ripple-island"
HEX = re.compile(r"^#[0-9a-fA-F]{6}$")
ROLES = ("background", "surface", "border", "dim", "text", "primary", "accent", "error")

sys.path.insert(0, sys.argv[1])
try:  # Resolve the agent's committed dependency environment (same as palette.py).
    from pm.environments import selected_venv, site_packages
    sys.path.insert(1, str(site_packages(selected_venv(Path(sys.argv[1])))))
except Exception:
    pass


def hermes_home() -> Path:
    try:
        from hermes_constants import get_hermes_home
        return Path(get_hermes_home())
    except Exception:
        return Path(os.environ.get("HERMES_HOME") or Path.home() / ".hermes")


def active_skin() -> str:
    try:
        from hermes_cli.config import load_config_readonly
        display = (load_config_readonly() or {}).get("display") or {}
    except Exception:
        display = {}
    name = display.get("skin") if isinstance(display, dict) else None
    return name.strip() if isinstance(name, str) and name.strip() else "default"


def load_yaml(path: Path):
    try:
        try:
            import hermes_yaml as yaml
        except Exception:
            import yaml
        with open(path, encoding="utf-8-sig") as f:
            data = yaml.safe_load(f)
        return data if isinstance(data, dict) else None
    except Exception:
        return None


def base_definition(name: str) -> dict:
    """The base skin as authored (user file first, then the built-in table), else empty."""
    data = load_yaml(hermes_home() / "skins" / f"{name}.yaml")
    if data is None:
        try:
            from hermes_cli import skin_engine
            data = copy.deepcopy(getattr(skin_engine, "_BUILTIN_SKINS", {}).get(name))
        except Exception:
            data = None
    return data if isinstance(data, dict) else {}


def mix(a: str, b: str, amount: float) -> str:
    pa = [int(a[i:i + 2], 16) for i in (1, 3, 5)]
    pb = [int(b[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * amount):02x}" for x, y in zip(pa, pb))


def skin_colors(p: dict) -> dict:
    bg, surface, border, dim, text, primary, accent, error = (p[r] for r in ROLES)
    return {
        "background": bg,
        "ui_accent": accent, "banner_accent": accent, "prompt": accent, "input_rule": accent,
        "ui_tool": accent, "ui_ok": accent, "status_bar_good": accent, "syntax_keyword": accent,
        "status_bar_strong": accent,
        "banner_title": primary, "ui_primary": primary,
        "banner_text": text, "ui_text": text, "ui_label": text, "status_bar_text": text,
        "banner_dim": dim, "syntax_comment": dim, "ui_thinking": dim, "status_bar_dim": dim,
        "banner_border": border, "ui_border": border, "session_border": border,
        "response_border": mix(border, accent, .35), "session_label": mix(text, accent, .5),
        "status_bar_bg": surface, "voice_status_bg": surface, "completion_menu_bg": surface,
        "completion_menu_meta_bg": mix(surface, bg, .5),
        "completion_menu_current_bg": mix(surface, accent, .22),
        "selection_bg": mix(surface, accent, .3),
        "ui_error": error, "status_bar_critical": error,
    }


def use_skin(launcher: str, name: str) -> None:
    env = dict(os.environ, NO_COLOR="1")
    for args in (["skin", "use", name], ["config", "set", "display.skin", name]):
        try:
            done = subprocess.run([launcher, *args], capture_output=True, timeout=30, env=env,
                                  stdin=subprocess.DEVNULL)
        except (OSError, subprocess.SubprocessError):
            continue
        if done.returncode == 0 and active_skin() == name:
            return
    raise SystemExit(json.dumps({"error": f"could not activate Hermes skin {name!r}"}))


def write_atomic(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def main() -> None:
    launcher, verb = sys.argv[2], sys.argv[3]
    path = hermes_home() / "skins" / f"{SKIN}.yaml"
    current = active_skin()
    existing = load_yaml(path) or {}
    remembered = existing.get("ripple_island_base")
    base = (remembered if isinstance(remembered, str) and remembered and remembered != SKIN else "default") \
        if current == SKIN else current
    if verb == "restore":
        if current == SKIN:
            use_skin(launcher, base)
            print(json.dumps({"skin": base, "changed": True}))
        else:
            print(json.dumps({"skin": current, "changed": False}))
        return
    if verb != "apply":
        raise SystemExit(json.dumps({"error": "unknown verb"}))
    palette = json.loads(sys.argv[4])
    if not all(isinstance(palette.get(r), str) and HEX.match(palette[r]) for r in ROLES):
        raise SystemExit(json.dumps({"error": "invalid palette"}))
    palette = {r: palette[r].lower() for r in ROLES}
    data = {k: v for k, v in base_definition(base).items() if k not in ("light_colors", "dark_colors")}
    raw_colors = data.get("colors")
    colors: dict = dict(raw_colors) if isinstance(raw_colors, dict) else {}
    data.update({
        "name": SKIN,
        "description": f"Ripple island colours over {base} (managed by Ripple; turn off Hermes theme sync to restore {base})",
        "colors": {**colors, **skin_colors(palette)},
        "ripple_island_base": base,
    })
    text = json.dumps(data, indent=2, ensure_ascii=True) + "\n"
    changed = not path.is_file() or path.read_text(encoding="utf-8", errors="replace") != text
    if changed:
        write_atomic(path, text)
    if current != SKIN:
        use_skin(launcher, SKIN)
        changed = True
    print(json.dumps({"skin": SKIN, "changed": changed}))


if __name__ == "__main__":
    main()
