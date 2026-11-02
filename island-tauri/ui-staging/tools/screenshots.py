#!/usr/bin/env python3
"""Capture built gallery stories with system WebKitGTK on a private Xvfb.
Refuse ANY browser/display startup while another agent's GUI gate is running.
No inherited display, bus, HOME, clipboard or service integration is used.
"""
import json
import hashlib
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import time
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'evidence/round2/screenshots'
PATTERN = r'kwin_wayland --virtual|phase[0-9a-z_]*-gate.sh'
STORIES = ['shell', 'grid', 'layout', 'timer', 'controls', 'dashboard', 'chips', 'calendar', 'scratchpad', 'downloads', 'mixer', 'brightness', 'notifications', 'clipboard', 'music', 'quickinfo', 'modules']
VARIANTS = {'calendar': ['agenda', 'unconfigured', 'stale', 'empty'], 'scratchpad': ['saving', 'error', 'empty'], 'downloads': ['empty'], 'mixer': ['empty', 'unavailable'], 'brightness': ['empty', 'unavailable'], 'notifications': ['empty', 'noActions'], 'clipboard': ['paused', 'empty'], 'music': ['plain', 'unavailable', 'noPlayer', 'paused'], 'quickinfo': ['unknownWeather', 'lowBattery', 'unavailable']}
SCENES = [(story, None) for story in STORIES] + [(story, fixture) for story, fixtures in VARIANTS.items() for fixture in fixtures]

def active_gates():
    result = subprocess.run(['pgrep', '-af', PATTERN], capture_output=True, text=True)
    return result.stdout.strip()

def parent():
    OUT.mkdir(parents=True, exist_ok=True)
    active = active_gates()
    if active:
        report = {'status': 'UNVERIFIED', 'reason': 'Other agent GUI gate active; no browser or display started', 'processes': active, 'enginePlanned': 'WebKitGTK 4.1 via PyGObject + private Xvfb'}
        (OUT / 'status.json').write_text(json.dumps(report, indent=2) + '\n')
        print('SKIPPED screenshots: other agent GUI gate active\n' + active)
        return 0
    print('GUI isolation preflight: pgrep produced no matches', flush=True)
    # Keep all caches/profile data in this package; no real user bus or display.
    cache = ROOT / '.cache/screenshots'
    for part in ['home', 'config', 'cache', 'data', 'runtime']:
        (cache / part).mkdir(parents=True, exist_ok=True)
    os.chmod(cache / 'runtime', 0o700)
    env = {k: v for k, v in os.environ.items() if k in ['PATH', 'LANG', 'TMPDIR', 'LD_LIBRARY_PATH']}
    env.update(HOME=str(cache / 'home'), XDG_CONFIG_HOME=str(cache / 'config'), XDG_CACHE_HOME=str(cache / 'cache'), XDG_DATA_HOME=str(cache / 'data'), XDG_RUNTIME_DIR=str(cache / 'runtime'), DBUS_SESSION_BUS_ADDRESS='disabled:', GDK_BACKEND='x11', WEBKIT_DISABLE_DMABUF_RENDERER='1', NO_AT_BRIDGE='1')
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
    # Static server only, no Vite dev server filesystem access.
    server = subprocess.Popen([sys.executable, '-m', 'http.server', str(port), '--bind', '127.0.0.1', '--directory', str(ROOT / 'dist')], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        for _ in range(50):
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.1): break
            except OSError: time.sleep(.1)
        # Recheck immediately before the display server/browser command.
        if active_gates():
            print('SKIPPED: GUI gate appeared before Xvfb startup')
            return 0
        print('GUI isolation immediately before Xvfb: pgrep produced no matches', flush=True)
        job = subprocess.Popen(['nice', '-n', '10', 'xvfb-run', '-a', '-s', '-screen 0 1100x1800x24', '/usr/bin/python3', str(pathlib.Path(__file__).resolve()), '--capture', str(port)], env=env, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
        try:
            output, _ = job.communicate(timeout=300)
        finally:
            # Exact process group created above, including a stalled WebKit child.
            try:
                os.killpg(job.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                job.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(job.pid, signal.SIGKILL)
                job.wait(timeout=5)
        (OUT / 'capture.log').write_text(output)
        print(output)
        if job.returncode:
            return job.returncode
        return 0
    finally:
        os.killpg(server.pid, signal.SIGTERM)
        server.wait(timeout=5)

def capture(port):
    assert not active_gates(), 'GUI gate appeared before browser startup'
    print('GUI isolation immediately before WebKitGTK: pgrep produced no matches', flush=True)
    import gi
    gi.require_version('Gtk', '3.0')
    gi.require_version('WebKit2', '4.1')
    from gi.repository import Gtk, WebKit2, GLib
    manager = WebKit2.WebsiteDataManager.new_ephemeral()
    context = WebKit2.WebContext.new_with_website_data_manager(manager)
    view = WebKit2.WebView.new_with_context(context)
    view.get_user_content_manager().add_script(WebKit2.UserScript.new('window.__captureErrors=[];window.addEventListener("error",e=>window.__captureErrors.push(e.message));window.addEventListener("unhandledrejection",e=>window.__captureErrors.push(String(e.reason)));', WebKit2.UserContentInjectedFrames.TOP_FRAME, WebKit2.UserScriptInjectionTime.START, None, None))
    win = Gtk.Window()
    win.set_default_size(1100, 1800)
    win.add(view)
    win.show_all()
    pending = [(theme, story, fixture) for theme in ['skin', 'neutral'] for story, fixture in SCENES]
    saved = []
    errors = []
    snapshots = {}
    def fatal(message):
        errors.append(str(message)); Gtk.main_quit(); return False
    def next_page():
        if not pending:
            Gtk.main_quit(); return False
        theme, story, fixture = pending.pop(0)
        view.load_uri(f'http://127.0.0.1:{port}/?theme={theme}&story={story}' + (f'&fixture={fixture}' if fixture else ''))
        return False
    def on_loaded(_view, event):
        if event != WebKit2.LoadEvent.FINISHED: return
        # Bounded settle wait: no animations in default stories. Confirm actual DOM
        # component and loaded fonts before snapshot; count is fail-closed.
        def inspect():
            view.evaluate_javascript('JSON.stringify({ready:document.querySelectorAll(".story").length===17 && [...document.querySelectorAll(".story")].filter(el=>getComputedStyle(el).display!=="none").length===1,fonts:document.fonts.status,fontLoaded:document.fonts.check("14px OpenRunde"),imagesReady:[...document.querySelectorAll(".bounded-image")].filter(el=>el.getClientRects().length).every(el=>el.complete && el.naturalWidth>0),height:document.querySelector("main").scrollHeight,errors:window.__captureErrors||[]})', -1, None, None, None, inspected, None)
            return False
        GLib.timeout_add(1200, inspect)
    def inspected(_view, result, _data):
        try:
            status = json.loads(view.evaluate_javascript_finish(result).to_string())
            if not status['ready'] or status['fonts'] != 'loaded' or not status['fontLoaded'] or not status['imagesReady'] or status['errors']: return fatal(status)
            view.get_snapshot(WebKit2.SnapshotRegion.FULL_DOCUMENT, WebKit2.SnapshotOptions.NONE, None, snapped, None)
        except Exception as e: fatal(e)
    def snapped(_view, result, _data):
        try:
            theme_match = re.search(r'theme=([^&]+)', view.get_uri())
            story_match = re.search(r'story=([^&]+)', view.get_uri())
            fixture_match = re.search(r'fixture=([^&]+)', view.get_uri())
            assert theme_match and story_match
            theme = theme_match[1]
            story = story_match[1]
            fixture = '-' + fixture_match[1] if fixture_match else ''
            file = OUT / f'{theme}-{story}{fixture}.png'
            surface = view.get_snapshot_finish(result)
            surface.write_to_png(str(file))
            assert file.stat().st_size > 10000, 'Suspiciously empty screenshot'
            digest = hashlib.sha256(file.read_bytes()).hexdigest()
            previous, attempts = snapshots.get(str(file), (None, 0))
            snapshots[str(file)] = (digest, attempts + 1)
            if previous != digest:
                assert attempts < 5, f'Unsettled screenshot: {file.name}'
                GLib.timeout_add(250, lambda: (view.get_snapshot(WebKit2.SnapshotRegion.FULL_DOCUMENT, WebKit2.SnapshotOptions.NONE, None, snapped, None), False)[1])
                return
            saved.append(str(file.relative_to(ROOT)))
            print(f'CAPTURED {file.relative_to(ROOT)}', flush=True)
            GLib.idle_add(next_page)
        except Exception as e: fatal(e)
    view.connect('load-changed', on_loaded)
    view.connect('load-failed', lambda _v, _e, uri, error: fatal(f'{uri}: {error}'))
    GLib.timeout_add_seconds(280, lambda: fatal('Capture deadline exceeded'))
    next_page(); Gtk.main(); win.destroy()
    if errors: raise RuntimeError(errors)
    assert len(saved) == len(SCENES) * 2
    report = {'status': 'captured', 'engine': f'WebKitGTK {WebKit2.get_major_version()}.{WebKit2.get_minor_version()}.{WebKit2.get_micro_version()}', 'paths': saved, 'settled': 'two consecutive byte-identical PNGs per scene, 250ms apart', 'isolation': 'ephemeral WebKit context, private Xvfb, private HOME/XDG, disabled session bus'}
    (OUT / 'status.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'PASS: {len(saved)} gallery screenshots, both themes')
    return 0
if __name__ == '__main__':
    raise SystemExit(capture(sys.argv[2]) if len(sys.argv) > 1 and sys.argv[1] == '--capture' else parent())
