"""Run the real palette adapter against a private Hermes home and fixture skin engine.

No installed Hermes imports, real configuration, or desktop services are used.
Run: python3 -B -m unittest discover -s island/tests -p test_palette.py -v
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HELPER = Path(__file__).resolve().parents[1] / "palette.py"

SHELL_ENGINE = """\
COLORS = {
    "default": {"background": "#111111", "ui_accent": "#dddddd", "ui_text": "#dddddd",
                "ui_primary": "#eeeeee", "banner_dim": "#999999", "ui_border": "#444444",
                "status_bar_bg": "#222222", "ui_error": "#ff8888"},
    "fixture-base": {"background": "#101020", "ui_accent": "#aaaaaa", "ui_text": "#eeeeee",
                     "ui_primary": "#cccccc", "banner_dim": "#999999", "ui_border": "#333333",
                     "status_bar_bg": "#202030", "ui_error": "#ff4444"},
    "monolith": {"background": "#080d0a", "ui_accent": "#80ff36", "ui_text": "#d8e2d7",
                 "ui_primary": "#e7eee4", "banner_dim": "#8a9c8d", "ui_border": "#3d6243",
                 "status_bar_bg": "#121c15", "ui_error": "#ff7272"},
    "ripple-island": {"background": "#101010", "ui_accent": "#dddddd", "ui_text": "#dddddd",
                      "ui_primary": "#eeeeee", "banner_dim": "#999999", "ui_border": "#444444",
                      "status_bar_bg": "#222222", "ui_error": "#ff8888"},
}
_given = {}


def init_skin_from_config(config):
    global _given
    _given = config or {}


def get_active_skin():
    display = _given.get("display") or {}
    skin_name = display.get("skin") or "default"
    class Skin:
        name = None
        def get_color(self, key, fallback):
            return COLORS.get(Skin.name, {}).get(key, fallback)
    Skin.name = skin_name[:80]
    return Skin()
"""


class PaletteTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="monolith-palette-", dir=os.environ.get("TMPDIR"))
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.home = self.root / "home"
        self.agent = self.root / "agent"
        (self.home / "skins").mkdir(parents=True)
        (self.agent / "hermes_cli").mkdir(parents=True)
        (self.agent / "hermes_cli" / "__init__.py").write_text("")
        (self.agent / "hermes_cli" / "config.py").write_text(
            "import json, os\nfrom pathlib import Path\n"
            "def load_config_readonly():\n"
            "    return json.loads((Path(os.environ['HERMES_HOME']) / 'config.json').read_text())\n"
        )
        (self.agent / "hermes_cli" / "skin_engine.py").write_text(SHELL_ENGINE)
        # palette.py resolves the agent's committed dependency env via pm.environments.
        (self.agent / "pm").mkdir(parents=True)
        (self.agent / "pm" / "__init__.py").write_text("")
        (self.agent / "pm" / "environments.py").write_text(
            "from pathlib import Path\n"
            "def selected_venv(root):\n    return root\n"
            "def site_packages(venv):\n    return venv / 'site-packages'\n"
        )
        (self.agent / "pm" / "site-packages").mkdir()
        self.config = self.home / "config.json"
        self.set_active("fixture-base")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("PYTHON") and k != "HERMES_HOME"}
        self.env.update(HOME=str(self.root), HERMES_HOME=str(self.home), PYTHONDONTWRITEBYTECODE="1")
        self.skin = self.home / "skins" / "monolith.yaml"
        self.legacy_skin = self.home / "skins" / "ripple-island.yaml"

    def set_active(self, name):
        self.config.write_text(json.dumps({"display": {"skin": name}}))

    def run_palette(self, sync):
        args = [sys.executable, "-B", str(HELPER), str(self.agent)]
        if sync:
            args.append("--theme-sync")
        return subprocess.run(args, env=self.env, capture_output=True, text=True, timeout=10)

    def palette(self, sync):
        result = self.run_palette(sync)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_with_sync_on_generated_skin_follows_remembered_base_under_new_name(self):
        self.skin.write_text(json.dumps({"monolith_base": "fixture-base"}))
        self.set_active("monolith")
        out = self.palette(sync=True)
        self.assertEqual(out["name"], "fixture-base")
        self.assertEqual(out["accent"], "#aaaaaa")

    def test_with_sync_on_legacy_generated_skin_is_still_recognized(self):
        self.legacy_skin.write_text(json.dumps({"ripple_island_base": "fixture-base"}))
        self.set_active("ripple-island")
        out = self.palette(sync=True)
        self.assertEqual(out["name"], "fixture-base")
        self.assertEqual(out["accent"], "#aaaaaa")

    def test_with_sync_on_and_no_remembered_base_falls_back_to_default(self):
        self.skin.write_text(json.dumps({"monolith_base": "monolith"}))
        self.set_active("monolith")
        out = self.palette(sync=True)
        self.assertEqual(out["name"], "default")

    def test_with_sync_off_ours_is_a_user_skin_not_swapped(self):
        self.skin.write_text(json.dumps({"monolith_base": "fixture-base"}))
        self.set_active("monolith")
        out = self.palette(sync=False)
        self.assertEqual(out["name"], "monolith")
        self.assertEqual(out["accent"], "#80ff36")

    def test_legacy_generated_skin_is_followed_as_is_when_sync_is_off(self):
        self.legacy_skin.write_text(json.dumps({"ripple_island_base": "fixture-base"}))
        self.set_active("ripple-island")
        out = self.palette(sync=False)
        self.assertEqual(out["name"], "ripple-island")
        self.assertEqual(out["accent"], "#dddddd")


if __name__ == "__main__":
    unittest.main()
