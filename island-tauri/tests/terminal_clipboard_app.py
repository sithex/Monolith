#!/usr/bin/env python3
"""Real release app + real keys + external clipboard read, in isolated Xvfb only.
Run: env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- /usr/bin/python3 tests/terminal_clipboard_app.py
"""
import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import gi

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gdk, GLib

assert os.environ.get('GDK_BACKEND') == 'x11' and not os.environ.get('WAYLAND_DISPLAY')
ROOT = Path(__file__).resolve().parents[1]
x11 = ctypes.CDLL('libX11.so.6')
xtst = ctypes.CDLL('libXtst.so.6')
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
x11.XFree.argtypes = [ctypes.c_void_p]
x11.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XFlush.argtypes = [ctypes.c_void_p]
xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
display = x11.XOpenDisplay(None)
assert display
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
context = GLib.MainContext.default()


def pump():
    while context.pending():
        context.iteration(False)
    time.sleep(0.02)


def wait(label, fn, timeout=20):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            last = fn()
            if last:
                return last
        except (OSError, AssertionError) as error:
            last = str(error)
        pump()
    raise AssertionError(f'{label}: timed out; last={last!r}')


with tempfile.TemporaryDirectory(prefix='monolith-copy-') as directory:
    base = Path(directory)
    env = os.environ.copy()
    for name in ['HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME']:
        path = base / name
        path.mkdir()
        env[name] = str(path)
    data = base / 'instance'
    data.mkdir()
    # Read-only palette adapter fixture; never run or load the user's Hermes install.
    agent = Path(env['HOME']) / '.hermes/hermes-agent'
    for name in ['pm', 'hermes_cli', '.hermes/bin']:
        (agent / name).mkdir(parents=True, exist_ok=True)
    (agent / '.hermes/bin/hermes').write_text('#!/bin/sh\nexec /usr/bin/python3\n')
    (agent / 'pm/environments.py').write_text('def selected_venv(root): return root\ndef site_packages(root): return root\n')
    (agent / 'hermes_cli/config.py').write_text('def load_config_readonly(): return {}\n')
    (agent / 'hermes_cli/skin_engine.py').write_text("def init_skin_from_config(config): pass\nclass Skin:\n name='Clipboard test'\n def get_color(self,key,fallback): return fallback\ndef get_active_skin(): return Skin()\n")
    env['HERMES_HOME'] = str(agent.parent)
    (data / 'settings.json').write_text(json.dumps({'clickSounds': False, 'displayBackend': 'x11'}))
    env.update(HERMES_ISLAND_TEST='1', NO_AT_BRIDGE='1', GTK_USE_PORTAL='0', GIO_USE_PORTAL='0')
    log = open(base / 'app.log', 'w+')
    app = subprocess.Popen([str(ROOT / 'src-tauri/target/release/hermes-island'), '--test-instance', '--fixture-pty', '--ui', 'island', '--data-dir', str(data)], env=env, stdout=log, stderr=log, start_new_session=True)
    try:
        control = data / 'test-control.sock'
        wait('app test socket', control.exists)

        def evaluate(source):
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(8)
                connection.connect(str(control))
                connection.sendall((json.dumps({'op': 'eval', 'js': source}) + '\n').encode())
                response = connection.makefile().readline()
            row = json.loads(response)
            assert row.get('ok'), row
            return row.get('value')

        wait('page bootstrap', lambda: evaluate('!!document.querySelector(".island")'))
        evaluate("window.__TAURI_INTERNALS__.invoke('island_dispatch',{intent:{type:'pin'}}).then(()=>true)")
        wait('terminal mounted', lambda: evaluate('!!window.__islandTerminalTest'))
        time.sleep(0.8)
        root = x11.XDefaultRootWindow(display)
        root_out, parent = ctypes.c_ulong(), ctypes.c_ulong()
        children, count = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
        assert x11.XQueryTree(display, root, ctypes.byref(root_out), ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count))
        windows = list(children[:count.value])
        x11.XFree(children)
        matches = [window for window in windows if f'= {app.pid}' in subprocess.check_output(['xprop', '-id', str(window), '_NET_WM_PID'], text=True, stderr=subprocess.DEVNULL)]
        assert matches, 'App X11 window not found'
        x11.XSetInputFocus(display, matches[0], 1, 0)
        x11.XFlush(display)
        clipboard.set_text('OLD-CLIPBOARD', -1)
        wait('old clipboard installed', lambda: clipboard.wait_for_text() == 'OLD-CLIPBOARD')
        marker = 'MONOLITH-COPY-VERIFIED'
        evaluate("new Promise(resolve=>{const t=window.__islandTerminalTest.identity;t.reset();t.write('MONOLITH-COPY-VERIFIED',()=>{t.select(0,0,22);t.focus();resolve(t.getSelection())})})")
        assert evaluate('window.__islandTerminalTest.identity.getSelection()') == marker

        def copy_key():
            for sym in [0xffe3, ord('c')]:
                xtst.XTestFakeKeyEvent(display, x11.XKeysymToKeycode(display, sym), 1, 0)
            for sym in [ord('c'), 0xffe3]:
                xtst.XTestFakeKeyEvent(display, x11.XKeysymToKeycode(display, sym), 0, 0)
            x11.XFlush(display)

        copy_key()
        wait('Ctrl+C replaces external clipboard', lambda: clipboard.wait_for_text() == marker)
        evaluate('(window.__islandTerminalTest.identity.clearSelection(),true)')
        copy_key()
        for _ in range(15):
            pump()
        assert clipboard.wait_for_text() == marker, 'Empty copy changed clipboard'
        print('PASS: release Monolith real Ctrl+C replaces old system clipboard with selected xterm text; empty copy preserves it')
    except Exception:
        try:
            print('Page diagnostic:', evaluate('({url:location.href,text:document.body.innerText})'))
        except Exception:
            pass
        log.flush()
        log.seek(0)
        print(log.read()[-4000:])
        raise
    finally:
        os.killpg(app.pid, signal.SIGTERM)
        app.wait(timeout=10)
        log.close()
