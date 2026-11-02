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
