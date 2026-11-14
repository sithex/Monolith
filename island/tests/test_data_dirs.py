"""Path policy tests run against private directories; no live island or XDG reads."""
import ast
import os
from pathlib import Path
import runpy
import tempfile
import unittest

CLIENT = Path(__file__).resolve().parents[1] / 'anchor-client.py'
resolve = runpy.run_path(str(CLIENT))['resolve_data_dirs']


class DataDirectoryTests(unittest.TestCase):
    def test_relative_xdg_values_fall_back_and_absolute_values_are_used(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            env = {'HOME': tmp, 'XDG_CONFIG_HOME': 'relative', 'XDG_STATE_HOME': ''}
            config, state, runtime = resolve(env, str(home / 'repo'))
            self.assertEqual(config, str(home / '.config/monolith'))
            self.assertEqual(state, str(home / '.local/state/monolith'))
            self.assertEqual(runtime, str(home / 'repo/.island-data-overlay'))
            env.update(XDG_CONFIG_HOME=str(home / 'config'), XDG_STATE_HOME=str(home / 'state'))
            self.assertEqual(resolve(env, str(home / 'repo'))[:2],
                             (str(home / 'config/monolith'), str(home / 'state/monolith')))

    def test_before_migration_and_failure_use_old_status_after_commit_uses_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            old = home / 'repo/.island-data-overlay'
            old.mkdir(parents=True)
            env = {'HOME': tmp}
            self.assertEqual(resolve(env, str(home / 'repo'))[1], str(old))
            state = home / '.local/state/monolith'
            state.mkdir(parents=True)
            # The pre-existing published helper link is not saved user data.
            (state / 'anchor-client.py').symlink_to(CLIENT)
            self.assertEqual(resolve(env, str(home / 'repo'))[1], str(old))
            (state / 'MIGRATED-FROM-REPO.json').write_text('{}')
            self.assertEqual(resolve(env, str(home / 'repo'))[1], str(state))
            (old / '.xdg-migration-failed').write_text('failure')
            self.assertEqual(resolve(env, str(home / 'repo'))[1], str(old))

    def test_click_path_keeps_same_lean_top_level_imports(self):
        imports = [n.names[0].name for n in ast.parse(CLIENT.read_text()).body if isinstance(n, ast.Import)]
        self.assertEqual(imports, ['os', 'sys', 'time', 'socket'])

    def test_plasmoid_bootstrap_ignores_relative_xdg_state_home(self):
        import json
        import subprocess
        qml = CLIENT.parent / 'plasma-anchor/contents/ui/main.qml'
        line = next(line for line in qml.read_text().splitlines() if 'property string command:' in line)
        command = json.loads(line.split('property string command:', 1)[1].strip())
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            for base, label in [(home / '.local/state', 'default'), (home / 'override', 'override')]:
                script = base / 'monolith/anchor-client.py'
                script.parent.mkdir(parents=True)
                script.write_text('import sys; print(' + repr(label) + ' + " " + sys.argv[1])')
            for value, expected in [('relative', 'default'), ('', 'default'), (str(home / 'override'), 'override')]:
                env = dict(PATH='/usr/bin:/bin', HOME=tmp, XDG_STATE_HOME=value)
                result = subprocess.run(['/bin/sh', '-c', command + ' status'], env=env,
                                        text=True, capture_output=True, check=True)
                self.assertEqual(result.stdout.strip(), expected + ' status')
