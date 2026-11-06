#!/usr/bin/env python3
"""End-to-end proof of the release disk-UI path, with an ISOLATED instance (own data dir, HOME,
X server and session bus; never touches the live island).

  env -u WAYLAND_DISPLAY -u DISPLAY xvfb-run -a -s '-screen 0 1600x1000x24' dbus-run-session -- \
    python3 tests/disk_ui_e2e.py <release hermes-island built from THIS checkout> <outdir>

Needs a release binary from `./build.sh` of this checkout (the instance serves this checkout's
island-tauri/dist). Edits ui/island/surface.css and ui/ipc/bindings.ts temporarily and restores
both, then re-deploys dist/ from the restored sources.

Checks: (1) the binary starts from the verified disk dist; (2) scripts/deploy-ui.sh with a changed
CSS value reloads the running webview and the screenshot shows it (no Rust rebuild, no restart,
same pid); (3) a contract change makes deploy-ui.sh refuse with 'needs ./build.sh'; (4) a tampered
dist file is rejected by a reload and by a fresh start, which falls back to the embedded UI and
still boots; (5) --embedded-ui forces the embedded UI.
"""
import json, os, shutil, socket, subprocess, sys, tempfile, time
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
BIN, OUT = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
# Optional read-only layout/appearance/settings to copy into the fixture (default: none, app defaults).
LIVE = Path(os.environ.get('MONOLITH_E2E_SEED_DIR', '/nonexistent'))
CSS = APP / 'ui/island/surface.css'
BINDINGS = APP / 'ui/ipc/bindings.ts'
OUT.mkdir(parents=True, exist_ok=True)
MAGENTA = 'rgb(255, 0, 255)'
failures = []


def check(name, ok, detail=''):
    print(('PASS ' if ok else 'FAIL ') + name + (f'  [{detail}]' if detail and not ok else ''), flush=True)
    if not ok:
        failures.append(name)


def wait(label, fn, timeout=25):
    end, last = time.monotonic() + timeout, None
    while time.monotonic() < end:
        try:
            last = fn()
            if last:
                return last
        except (OSError, AssertionError, ValueError) as e:
            last = str(e)
        time.sleep(0.1)
    raise AssertionError(f'{label}: timed out; last={last!r}')


class Instance:
    def __init__(self, base, name, *extra):
        self.env = os.environ.copy()
        for n in ['HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME']:
            (base / name / n).mkdir(parents=True)
            self.env[n] = str(base / name / n)
        self.data = base / name / 'instance'
        self.data.mkdir()
        agent = Path(self.env['HOME']) / '.hermes/hermes-agent'
        for d in ['pm', 'hermes_cli', '.hermes/bin']:
            (agent / d).mkdir(parents=True, exist_ok=True)
        (agent / '.hermes/bin/hermes').write_text('#!/bin/sh\nexec /usr/bin/python3\n')
        (agent / 'pm/environments.py').write_text('def selected_venv(root): return root\ndef site_packages(root): return root\n')
        (agent / 'hermes_cli/config.py').write_text('def load_config_readonly(): return {}\n')
        (agent / 'hermes_cli/skin_engine.py').write_text("def init_skin_from_config(config): pass\nclass Skin:\n name='E2E'\n def get_color(self,key,fallback): return fallback\ndef get_active_skin(): return Skin()\n")
        self.env['HERMES_HOME'] = str(agent.parent)
        for f in ['layout.json', 'appearance.json']:
            if (LIVE / f).exists():
                shutil.copy(LIVE / f, self.data / f)
        settings = json.loads((LIVE / 'settings.json').read_text()) if (LIVE / 'settings.json').exists() else {}
        settings.update(clickSounds=False, displayBackend='x11')
        (self.data / 'settings.json').write_text(json.dumps(settings))
        self.env.update(HERMES_ISLAND_TEST='1', NO_AT_BRIDGE='1', GTK_USE_PORTAL='0', GIO_USE_PORTAL='0', GDK_BACKEND='x11')
        self.env.pop('WAYLAND_DISPLAY', None)
        self.logpath = base / name / 'app.log'
        self.log = open(self.logpath, 'w+')
        self.app = subprocess.Popen([str(BIN), '--test-instance', '--fixture-pty', '--ui', 'island', '--data-dir', str(self.data), *extra],
                                    env=self.env, stdout=self.log, stderr=self.log, start_new_session=True)
        self.sock = self.data / 'test-control.sock'
        wait('test socket', self.sock.exists)
        wait('page', lambda: self.ev('!!document.querySelector(".island")'))
        time.sleep(1.5)

    def ev(self, js):
        with socket.socket(socket.AF_UNIX) as c:
            c.settimeout(8)
            c.connect(str(self.sock))
            c.sendall((json.dumps({'op': 'eval', 'js': js}) + '\n').encode())
            row = json.loads(c.makefile().readline())
        assert row.get('ok'), row
        return row.get('value')

    def dispatch(self, intent):
        self.ev(f"window.__TAURI_INTERNALS__.invoke('island_dispatch',{{intent:{json.dumps(intent)}}}).then(()=>true)")

    def panel(self, line=b'reload-ui\n'):
        with socket.socket(socket.AF_UNIX) as c:
            c.settimeout(10)
            c.connect(str(self.data / 'panel.sock'))
            c.sendall(line)
            c.shutdown(socket.SHUT_WR)
            data = b''
            while chunk := c.recv(65536):
                data += chunk
        return json.loads(data)

    def shot(self, name):
        subprocess.run(['import', '-window', 'root', str(OUT / name)], check=True)

    def text(self):
        self.log.flush()
        return self.logpath.read_text(errors='replace')

    def stop(self):
        self.app.terminate()
        try:
            self.app.wait(10)
        except subprocess.TimeoutExpired:
            self.app.kill()


def deploy(inst, *args):
    env = dict(os.environ, HERMES_ISLAND_BIN=str(BIN), HERMES_ISLAND_DATA=str(inst.data))
    return subprocess.run([str(APP / 'scripts/deploy-ui.sh'), *args], env=env, capture_output=True, text=True, cwd=APP)


css_orig, bindings_orig = CSS.read_text(), BINDINGS.read_text()
with tempfile.TemporaryDirectory(prefix='disk-ui-e2e-', dir=os.environ.get('TMPDIR')) as d:
    base = Path(d)
    inst: Instance
    try:
        # (1) starts from the verified disk dist
        inst = Instance(base, 'a')
        check('starts from the verified disk dist', 'serving the verified dist/ from disk' in inst.text(), inst.text()[-400:])
        inst.dispatch({'type': 'pin'})
        wait('terminal', lambda: inst.ev('!!window.__islandTerminalTest'))
        inst.dispatch({'type': 'selectView', 'view': 'settings'})
        time.sleep(1.5)
        style = '''(()=>{const e=document.querySelector(".island");return getComputedStyle(e).outlineColor+"|"+getComputedStyle(e).outlineWidth})()'''
        before = inst.ev(style)
        check('baseline has no magenta outline', MAGENTA not in before, before)
        inst.shot('1-before.png')
        pid = inst.app.pid
        inst.ev('(()=>{window.__e2eMarker=1;return true})()')

        # (2) change a CSS value, deploy, no restart
        CSS.write_text(css_orig + '\n.island { outline: 6px solid #ff00ff !important; outline-offset: -6px; }\n')
        t0 = time.monotonic()
        r = deploy(inst)
        took = time.monotonic() - t0
        print(r.stdout.strip(), r.stderr.strip(), flush=True)
        check('deploy-ui.sh succeeds', r.returncode == 0, r.stderr[-300:])
        check('deploy-ui.sh replied ok:true', '"ok":true' in r.stdout, r.stdout)
        check('deploy-ui.sh took under 60 s without Rust', took < 60, f'{took:.1f}s')
        wait('reloaded page', lambda: inst.ev('!!document.querySelector(".island") && window.__e2eMarker===undefined'))
        after = wait('new style', lambda: (lambda v: v if MAGENTA in v else None)(inst.ev(style)))
        check('reloaded webview shows the new CSS value', MAGENTA in after, after)
        check('same island process (not restarted)', inst.app.poll() is None and inst.app.pid == pid)
        time.sleep(1.5)
        inst.shot('2-after-deploy.png')
        wait('terminal after reload', lambda: inst.ev('!!window.__islandTerminalTest'))
        check('log says reloaded from verified dist', 'reloaded the island from the verified dist' in inst.text())

        # (3) a contract change must make deploy refuse
        BINDINGS.write_text(bindings_orig + '\n// e2e: contract drift\n')
        r = deploy(inst)
        check('contract mismatch: deploy-ui.sh refuses with needs ./build.sh', r.returncode == 3 and 'needs ./build.sh' in r.stderr, f'{r.returncode} {r.stderr[-200:]}')
        BINDINGS.write_text(bindings_orig)

        # (4) tamper with dist: reload refused, page keeps working; fresh start falls back to embedded
        victim = next((APP / 'dist/assets').glob('island-*.css'))
        victim.write_text(victim.read_text() + '\n/* tampered */')
        reply = inst.panel()
        check('tampered dist: reload refused', reply.get('ok') is False and 'does not match the manifest' in reply.get('error', ''), reply)
        check('tampered dist: running page untouched', inst.ev('!!document.querySelector(".island")') is True)
        inst.stop()
        inst2 = Instance(base, 'b')
        check('tampered dist: fresh start falls back to embedded', 'embedded assets (disk UI rejected' in inst2.text(), inst2.text()[-400:])
        check('embedded fallback still boots the island', inst2.ev('!!document.querySelector(".island")') is True)
        inst2.shot('3-embedded-fallback.png')
        inst2.stop()

        # (5) escape hatch
        inst3 = Instance(base, 'c', '--embedded-ui')
        check('--embedded-ui forces the embedded UI', 'embedded assets (forced by --embedded-ui)' in inst3.text(), inst3.text()[-300:])
        check('forced embedded boots', inst3.ev('!!document.querySelector(".island")') is True)
        inst3.stop()
    finally:
        CSS.write_text(css_orig)
        BINDINGS.write_text(bindings_orig)
        for i in [x for x in locals().values() if isinstance(x, Instance)]:
            i.stop()
        r = subprocess.run([str(APP / 'scripts/deploy-ui.sh'), '--no-reload'], env=dict(os.environ, HERMES_ISLAND_BIN=str(BIN)), capture_output=True, text=True, cwd=APP)
        print('restore dist:', r.returncode, r.stdout.strip(), r.stderr.strip()[-200:])
print('FAILED: ' + ', '.join(failures) if failures else 'ALL PASSED')
sys.exit(1 if failures else 0)
