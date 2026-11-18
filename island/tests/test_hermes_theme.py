"""Run the real theme helper against a private Hermes home and fixture CLI.

No installed Hermes imports, real configuration, or desktop services are used.
Run: python3 -B -m unittest discover -s island/tests -p test_hermes_theme.py -v
"""
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest


HELPER = Path(__file__).resolve().parents[1] / "hermes-theme.py"
PALETTE = dict(zip(
    ("background", "surface", "border", "dim", "text", "primary", "accent", "error"),
    ("#101020", "#202030", "#303040", "#808090", "#FAFAFA", "#CCCCEE", "#00FF80", "#FF0044"),
))


class HermesThemeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="monolith-theme-", dir=os.environ.get("TMPDIR"))
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
        # Fixtures are JSON (a YAML subset); no installed Hermes/YAML dependency is needed.
        (self.agent / "hermes_yaml.py").write_text("import json\nsafe_load = json.load\n")
        self.config = self.home / "config.json"
        self.set_active("fixture-base")
        self.base = {
            "name": "fixture-base", "branding": {"banner": "keep me"},
            "spinner": {"frames": ["a", "b"]}, "art": "fixture art",
            "colors": {"custom_role": "#123456", "ui_text": "#abcdef"},
            "light_colors": {"ui_text": "#ffffff"}, "dark_colors": {"ui_text": "#000000"},
        }
        self.base_path = self.home / "skins" / "fixture-base.yaml"
        self.base_path.write_text(json.dumps(self.base))
        self.launcher = self.root / "fixture-hermes"
        self.launcher.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\nfrom pathlib import Path\n"
            "home = Path(os.environ['HERMES_HOME'])\nargs = sys.argv[1:]\n"
            "with (home / 'calls.jsonl').open('a') as f: f.write(json.dumps(args) + '\\n')\n"
            "if os.environ.get('FAIL_ALL') or (os.environ.get('FAIL_SKIN') and args[:2] == ['skin', 'use']): sys.exit(1)\n"
            "if args[:2] != ['skin', 'use'] and args[:3] != ['config', 'set', 'display.skin']: sys.exit(2)\n"
            "if not os.environ.get('LIE_ABOUT_SUCCESS'):\n"
            "    (home / 'config.json').write_text(json.dumps({'display': {'skin': args[-1]}}))\n"
        )
        self.launcher.chmod(0o700)
        # Remove inherited Python import paths; script inserts only our fixture agent.
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("PYTHON") and k != "HERMES_HOME"}
        self.env.update(HOME=str(self.root), HERMES_HOME=str(self.home), PYTHONDONTWRITEBYTECODE="1")
        self.skin = self.home / "skins" / "monolith.yaml"
        self.legacy_skin = self.home / "skins" / "ripple-island.yaml"

    def set_active(self, name):
        self.config.write_text(json.dumps({"display": {"skin": name}}))

    def active(self):
        return json.loads(self.config.read_text())["display"]["skin"]

    def calls(self):
        path = self.home / "calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def run_helper(self, verb="apply", palette=None, **env):
        args = [sys.executable, "-B", str(HELPER), str(self.agent), str(self.launcher), verb]
        if verb == "apply":
            args.append(json.dumps(PALETTE if palette is None else palette))
        return subprocess.run(args, env={**self.env, **env}, capture_output=True, text=True, timeout=10)

    def success(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_apply_copies_branding_replaces_colors_and_is_idempotent(self):
        original = self.base_path.read_bytes()
        self.assertEqual(self.success(self.run_helper()), {"skin": "monolith", "changed": True})
        skin = json.loads(self.skin.read_text())
        self.assertEqual(self.active(), "monolith")
        self.assertEqual(skin["monolith_base"], "fixture-base")
        for key in ("branding", "spinner", "art"):
            self.assertEqual(skin[key], self.base[key])
        self.assertNotIn("light_colors", skin)
        self.assertNotIn("dark_colors", skin)
        self.assertEqual(skin["colors"]["custom_role"], "#123456")
        for role, expected in {"background": "#101020", "status_bar_bg": "#202030", "ui_text": "#fafafa", "ui_accent": "#00ff80", "ui_error": "#ff0044"}.items():
            self.assertEqual(skin["colors"][role], expected)
        self.assertEqual(stat.S_IMODE(self.skin.stat().st_mode), 0o600)
        before = (self.skin.read_bytes(), self.skin.stat().st_mtime_ns)
        self.assertEqual(self.success(self.run_helper()), {"skin": "monolith", "changed": False})
        self.assertEqual((self.skin.read_bytes(), self.skin.stat().st_mtime_ns), before)
        self.assertEqual(self.calls(), [["skin", "use", "monolith"]])
        self.assertEqual(self.base_path.read_bytes(), original)
        self.assertFalse(self.legacy_skin.exists())
        self.assertEqual(list(self.skin.parent.glob("*.tmp")), [])

    def test_light_community_palette_becomes_a_complete_skin(self):
        # Catppuccin Latte (ui/island/tokens/schemes.ts): a light background must pass the
        # same validation and map every skin role, with no dark_colors/light_colors override.
        latte = {"name": "monolith", "background": "#eff1f5", "surface": "#ccd0da", "border": "#bcc0cc",
                 "dim": "#6c6f85", "text": "#4c4f69", "primary": "#1e66f5", "accent": "#8839ef", "error": "#d20f39"}
        self.assertEqual(self.success(self.run_helper(palette=latte)), {"skin": "monolith", "changed": True})
        colors = json.loads(self.skin.read_text())["colors"]
        for role, expected in {"background": "#eff1f5", "ui_accent": "#8839ef", "ui_text": "#4c4f69", "ui_primary": "#1e66f5",
                               "ui_error": "#d20f39", "status_bar_bg": "#ccd0da", "ui_border": "#bcc0cc", "banner_dim": "#6c6f85"}.items():
            self.assertEqual(colors[role], expected, role)
        self.assertNotIn("light_colors", json.loads(self.skin.read_text()))

    def test_reapply_remembers_original_and_restore_reactivates_it(self):
        self.success(self.run_helper())
        self.success(self.run_helper(palette={**PALETTE, "accent": "#123ABC"}))
        self.assertEqual(json.loads(self.skin.read_text())["monolith_base"], "fixture-base")
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "fixture-base", "changed": True})
        self.assertEqual(self.active(), "fixture-base")
        count = len(self.calls())
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "fixture-base", "changed": False})
        self.assertEqual(len(self.calls()), count)

    def test_restore_does_not_override_a_manually_selected_skin(self):
        self.success(self.run_helper())
        self.set_active("manual-choice")
        count = len(self.calls())
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "manual-choice", "changed": False})
        self.assertEqual(self.active(), "manual-choice")
        self.assertEqual(len(self.calls()), count)

    def test_missing_remembered_base_restores_default(self):
        self.set_active("monolith")
        self.skin.write_text(json.dumps({"monolith_base": "monolith"}))
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "default", "changed": True})
        self.assertEqual(self.active(), "default")

    def test_remembered_base_never_resolves_to_the_legacy_generated_name(self):
        self.set_active("monolith")
        self.skin.write_text(json.dumps({"monolith_base": "ripple-island"}))
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "default", "changed": True})
        self.assertEqual(self.active(), "default")

    def test_invalid_palette_does_not_write_or_activate(self):
        for value in ("#abc", "red", "#gggggg", 42, None):
            with self.subTest(value=value):
                result = self.run_helper(palette={**PALETTE, "accent": value})
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("invalid palette", result.stderr)
                self.assertFalse(self.skin.exists())
                self.assertFalse(self.legacy_skin.exists())
                self.assertEqual(self.active(), "fixture-base")
                self.assertEqual(self.calls(), [])
        incomplete = dict(PALETTE)
        del incomplete["text"]
        self.assertNotEqual(self.run_helper(palette=incomplete).returncode, 0)
        self.assertFalse(self.skin.exists())

    def test_cli_fallback_is_used_and_activation_is_read_back(self):
        self.success(self.run_helper(FAIL_SKIN="1"))
        self.assertEqual(self.calls(), [["skin", "use", "monolith"], ["config", "set", "display.skin", "monolith"]])
        self.assertEqual(self.active(), "monolith")

    def test_success_exit_without_actual_activation_is_rejected(self):
        result = self.run_helper(LIE_ABOUT_SUCCESS="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("could not activate", result.stderr)
        self.assertEqual(self.active(), "fixture-base")
        self.assertEqual(len(self.calls()), 2)

    # ---- Migration from the old generated skin name (ripple-island -> monolith) ----

    def test_apply_from_active_legacy_skin_migrates_carries_base_and_deletes_old(self):
        self.legacy_skin.write_text(json.dumps({"ripple_island_base": "fixture-base"}))
        self.set_active("ripple-island")
        result = self.success(self.run_helper())
        self.assertEqual(result, {"skin": "monolith", "changed": True})
        # New generated skin exists, remembers the old file's base, and is active.
        self.assertTrue(self.skin.exists())
        skin = json.loads(self.skin.read_text())
        self.assertEqual(skin["monolith_base"], "fixture-base")
        self.assertNotIn("ripple_island_base", skin)
        self.assertEqual(self.active(), "monolith")
        # The old generated file is gone and the switch went through the CLI.
        self.assertFalse(self.legacy_skin.exists())
        self.assertEqual(self.calls(), [["skin", "use", "monolith"]])
        # The migrated skin still carries the base's branding and the new colours.
        for key in ("branding", "spinner", "art"):
            self.assertEqual(skin[key], self.base[key])
        self.assertEqual(skin["colors"]["ui_accent"], "#00ff80")

    def test_apply_migrates_even_when_the_new_skin_already_exists(self):
        # A previous run wrote monolith.yaml but the old file survived (or the machine was
        # mid-migration): applying again must still drop the old file and keep the base.
        self.legacy_skin.write_text(json.dumps({"ripple_island_base": "fixture-base"}))
        self.set_active("monolith")
        self.success(self.run_helper())
        self.assertFalse(self.legacy_skin.exists())
        self.assertEqual(json.loads(self.skin.read_text())["monolith_base"], "fixture-base")

    def test_restore_while_active_on_legacy_skin_restores_remembered_base(self):
        self.legacy_skin.write_text(json.dumps({"ripple_island_base": "fixture-base"}))
        self.set_active("ripple-island")
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "fixture-base", "changed": True})
        self.assertEqual(self.active(), "fixture-base")

    def test_restore_while_active_on_legacy_skin_with_no_base_restores_default(self):
        self.legacy_skin.write_text(json.dumps({"ripple_island_base": "ripple-island"}))
        self.set_active("ripple-island")
        self.assertEqual(self.success(self.run_helper("restore")), {"skin": "default", "changed": True})
        self.assertEqual(self.active(), "default")


if __name__ == "__main__":
    unittest.main()
