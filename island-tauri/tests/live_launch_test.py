"""Launcher plumbing tests use scratch executables, not the user's app or environment."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class LiveLauncherTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='hi-9a-launcher-', dir=os.environ['TMPDIR'])
        self.repo = Path(self.tmp.name)
        self.app = self.repo / 'island-tauri'
        self.app.mkdir()
        for name in ('run.sh', 'build.sh'):
            shutil.copyfile(ROOT / name, self.app / name)
            (self.app / name).chmod(0o700)
        self.bin = self.app / 'src-tauri/target/release/hermes-island'
        self.bin.parent.mkdir(parents=True)
        self.bin.write_text('#!/usr/bin/python3\nimport os,sys,json\nprint(json.dumps({"args":sys.argv[1:],"cwd":os.getcwd(),"gdk":os.environ.get("GDK_BACKEND"),"node":os.environ.get("NODE_OPTIONS"),"electron":os.environ.get("ELECTRON_RUN_AS_NODE")}))\n')
        self.bin.chmod(0o700)

    def tearDown(self):
        self.tmp.cleanup()

    def launch(self, *args):
        env = {'PATH': '/usr/bin:/bin', 'GDK_BACKEND': 'wayland',
               'HOME': str(self.repo / 'home'), 'XDG_CONFIG_HOME': str(self.repo / 'config'),
               'XDG_STATE_HOME': str(self.repo / 'state'),
               'NODE_OPTIONS': 'hostile-test', 'ELECTRON_RUN_AS_NODE': '1'}
        result = subprocess.run([str(self.app / 'run.sh'), *args], env=env,
                                cwd=self.repo, capture_output=True, text=True, check=True)
        return json.loads(result.stdout)

    def test_run_forces_x11_and_scrubs_electron_environment(self):
        row = self.launch('--overlay', '--test-instance', '--data-dir', '/scratch')
        self.assertEqual(row['gdk'], 'x11')
        self.assertIsNone(row['node'])
        self.assertIsNone(row['electron'])
        self.assertEqual(row['cwd'], str(self.repo))

    def test_overlay_default_selects_island_without_overriding_explicit_ui(self):
        self.assertEqual(self.launch('--overlay')['args'], ['--overlay', '--ui', 'island'])
        for args in [('--overlay', '--ui', 'legacy'), ('--overlay', '--ui=harness'), ()]:
            self.assertEqual(self.launch(*args)['args'], list(args))

    def test_test_override_reads_its_settings_and_never_writes_live_log(self):
        scratch = self.repo / 'instance'
        scratch.mkdir()
        (scratch / 'settings.json').write_text('{"displayBackend":"x11"}')
        config = self.repo / 'config/monolith'
        state = self.repo / 'state/monolith'
        config.mkdir(parents=True)
        state.mkdir(parents=True)
        (config / 'settings.json').write_text('{"displayBackend":"wayland"}')
        (state / 'monolith.log').write_text('untouched')
        env = dict(PATH='/usr/bin:/bin', HOME=str(self.repo / 'home'),
                   XDG_CONFIG_HOME=str(config.parent), XDG_STATE_HOME=str(state.parent),
                   WAYLAND_DISPLAY='test-only-no-display-connection')
        for args in [('--data-dir', str(scratch)), ('--data-dir=' + str(scratch),)]:
            result = subprocess.run([str(self.app / 'run.sh'), '--overlay', '--test-instance', *args],
                                    env=env, capture_output=True, text=True, check=True)
            self.assertEqual(json.loads(result.stdout)['gdk'], 'x11')
            self.assertEqual((state / 'monolith.log').read_text(), 'untouched')
            self.assertFalse((state / 'monolith.log.1').exists())
            self.assertFalse((self.repo / '.island-data-overlay').exists())

    def test_invalid_or_live_test_overrides_fail_before_settings_or_log_access(self):
        config = self.repo / 'config/monolith'
        state = self.repo / 'state/monolith'
        config.mkdir(parents=True)
        state.mkdir(parents=True)
        (config / 'settings.json').write_text('{"displayBackend":"wayland"}')
        (state / 'monolith.log').write_text('untouched')
        alias = self.repo / 'alias'
        alias.symlink_to(config, target_is_directory=True)
        env = dict(PATH='/usr/bin:/bin', HOME=str(self.repo / 'home'),
                   XDG_CONFIG_HOME=str(config.parent), XDG_STATE_HOME=str(state.parent),
                   WAYLAND_DISPLAY='test-only-no-display-connection')
        for args in [('--data-dir=',), ('--test-instance', '--data-dir='),
                     ('--test-instance', '--data-dir', str(config)),
                     ('--test-instance', '--data-dir', str(alias)),
                     ('--test-instance', '--data-dir', str(state / 'nested'))]:
            result = subprocess.run([str(self.app / 'run.sh'), '--overlay', *args], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, args)
            self.assertEqual(result.stdout, '')
            self.assertEqual((state / 'monolith.log').read_text(), 'untouched')
            self.assertFalse((state / 'monolith.log.1').exists())
        self.assertFalse((self.repo / '.island-data-overlay').exists())

    def test_live_launcher_uses_xdg_settings_log_and_private_modes(self):
        config = self.repo / 'config/monolith'
        state = self.repo / 'state/monolith'
        config.mkdir(parents=True)
        (config / 'settings.json').write_text('{"displayBackend":"x11"}')
        env = dict(PATH='/usr/bin:/bin', HOME=str(self.repo / 'home'),
                   XDG_CONFIG_HOME=str(config.parent), XDG_STATE_HOME=str(state.parent),
                   WAYLAND_DISPLAY='test-only-no-display-connection')
        result = subprocess.run([str(self.app / 'run.sh'), '--overlay'], env=env,
                                capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout)['gdk'], 'x11')
        self.assertEqual((state.stat().st_mode & 0o777), 0o700)
        self.assertEqual(((state / 'monolith.log').stat().st_mode & 0o777), 0o600)
        self.assertFalse((self.repo / '.island-data-overlay').exists())

    def test_release_build_explicitly_embeds_assets_and_never_installs_dependencies(self):
        tools = self.repo / 'bin'
        tools.mkdir()
        log = self.repo / 'calls.jsonl'
        for name in ('npm', 'cargo'):
            p = tools / name
            p.write_text('#!/usr/bin/python3\nimport sys,json\nfrom pathlib import Path\nwith Path(' + repr(str(log)) + ').open("a") as f:f.write(json.dumps([Path(sys.argv[0]).name,*sys.argv[1:]])+"\\n")\n')
            p.chmod(0o700)
        (self.app / 'src-tauri/build.rs').touch()
        subprocess.run([str(self.app / 'build.sh')], env={'PATH': f'{tools}:/usr/bin:/bin'}, check=True)
        rows = [json.loads(l) for l in log.read_text().splitlines()]
        self.assertEqual(rows[0], ['npm', 'run', 'build:ui'])
        self.assertEqual(rows[1], ['cargo', 'build', '--manifest-path', 'src-tauri/Cargo.toml', '--release', '--locked', '--features', 'custom-protocol'])
        self.assertEqual(len(rows), 2)


if __name__ == '__main__':
    unittest.main()
