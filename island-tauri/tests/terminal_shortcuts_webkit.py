#!/usr/bin/env python3
"""Real clipboard/key regression in isolated Xvfb, never the user's session.
Run: env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a /usr/bin/python3 tests/terminal_shortcuts_webkit.py
"""
import ctypes
import json
import subprocess
from pathlib import Path
import gi

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, Gdk, WebKit2, GLib

ROOT = Path(__file__).resolve().parents[1]
helper = subprocess.check_output(['node', '-e', "const ts=require('typescript'),fs=require('fs');console.log(ts.transpile(fs.readFileSync('ui/views/terminal-shortcuts.ts','utf8'),{module:ts.ModuleKind.ES2022,target:ts.ScriptTarget.ES2022}).replace(/^export /gm,''))"], cwd=ROOT, text=True)
# transpileModule leaves an export marker for ES modules; expose only the function.
helper = helper.replace('export {};', '')
x11 = ctypes.CDLL('libX11.so.6')
xtst = ctypes.CDLL('libXtst.so.6')
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XFlush.argtypes = [ctypes.c_void_p]
xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
display = x11.XOpenDisplay(None)
assert display, 'Xvfb display unavailable'

def key(keysym, shift=False):
    modifiers = [0xffe3] + ([0xffe1] if shift else [])
    for sym in modifiers + [keysym]:
        xtst.XTestFakeKeyEvent(display, x11.XKeysymToKeycode(display, sym), 1, 0)
    for sym in reversed(modifiers + [keysym]):
        xtst.XTestFakeKeyEvent(display, x11.XKeysymToKeycode(display, sym), 0, 0)
    x11.XFlush(display)

def drag():
    # Real pointer drag across the terminal while the app owns the mouse (TUI selection).
    _, ox, oy = window.get_window().get_origin()
    xtst.XTestFakeMotionEvent(display, -1, ox + 30, oy + 20, 0)
    xtst.XTestFakeButtonEvent(display, 1, 1, 0)
    for step in range(1, 9):
        xtst.XTestFakeMotionEvent(display, -1, ox + 30 + step * 20, oy + 20, 10)
    xtst.XTestFakeButtonEvent(display, 1, 0, 10)
    x11.XFlush(display)

manager = WebKit2.UserContentManager()
manager.register_script_message_handler('audit')
view = WebKit2.WebView.new_with_user_content_manager(manager)
window = Gtk.Window()
window.add(view)
window.set_default_size(700, 400)
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
errors = []
completed = []

def js(source):
    view.evaluate_javascript(source, -1, None, None, None, None, None)

def received(_manager, message):
    row = json.loads(message.get_js_value().to_string())
    stage = row.get('stage')
    try:
        if stage == 'native-copy':
            clipboard.set_text(row['text'], -1)
        elif stage == 'ready':
            key(ord('c'))
            GLib.timeout_add(250, copied)
        elif stage == 'pasted':
            assert row['data'] == ['\x1b[200~paste-probe\rnext\x1b[201~'], row
            js('term.clearSelection(); sent=[];')
            key(ord('c'))
            GLib.timeout_add(150, lambda: (js("audit({stage:'no-selection',data:sent})"), False)[1])
        elif stage == 'no-selection':
            assert row['data'] == [], row
            key(ord('c'), shift=True)
            GLib.timeout_add(150, lambda: (js("audit({stage:'interrupt',data:sent})"), False)[1])
        elif stage == 'interrupt':
            assert row['data'] == ['\x03'], row
            js("sent=[]; term.write('\\x1b[?1002h\\x1b[?1006h',()=>audit({stage:'mouse-ready'}))")
        elif stage == 'mouse-ready':
            drag()
            GLib.timeout_add(250, lambda: (js("audit({stage:'dragged',data:sent,selection:term.getSelection()})"), False)[1])
        elif stage == 'dragged':
            assert any(d.startswith('\x1b[<') for d in row['data']), row
            assert row['selection'] == '', row
            js('sent=[];')
            key(ord('c'))
            key(ord('c'))
            GLib.timeout_add(200, lambda: (js("audit({stage:'app-copy',data:sent})"), False)[1])
        elif stage == 'app-copy':
            assert row['data'] == ['\x03'], row
            clipboard.set_text('before-osc52', -1)
            js("sent=[];term.write('\\x1b]52;c;'+btoa('OSC52-MARKER')+'\\x07\\x1b]52;c;?\\x07',()=>setTimeout(()=>audit({stage:'osc52',data:sent}),100))")
        elif stage == 'osc52':
            assert row['data'] == [], row  # the clipboard query is never answered
            text = clipboard.wait_for_text()
            assert text == 'OSC52-MARKER', repr(text)
            completed.append(True)
            Gtk.main_quit()
        elif stage == 'error':
            raise AssertionError(row)
    except Exception as error:
        errors.append(str(error))
        Gtk.main_quit()

def copied():
    text = clipboard.wait_for_text()
    if text != 'COPY-MARKER':
        errors.append('Native Ctrl+C clipboard mismatch: ' + repr(text))
        Gtk.main_quit()
        return False
    clipboard.set_text('paste-probe\nnext', -1)
    js("term.clearSelection(); sent=[]; term.write('\\x1b[?2004h',()=>audit({stage:'paste-ready'}));")
    GLib.timeout_add(150, paste)
    return False

def paste():
    key(ord('v'))
    GLib.timeout_add(250, lambda: (js("audit({stage:'pasted',data:sent})"), False)[1])
    return False

manager.connect('script-message-received::audit', received)
window.show_all()
window.present()
view.grab_focus()
html = '<!doctype html><link rel="stylesheet" href="' + (ROOT/'node_modules/@xterm/xterm/css/xterm.css').as_uri() + '"><div id="term"></div><script src="' + (ROOT/'node_modules/@xterm/xterm/lib/xterm.js').as_uri() + '"></script><script>' + helper + "\nconst audit=row=>window.webkit.messageHandlers.audit.postMessage(JSON.stringify(row)); window.onerror=msg=>audit({stage:'error',msg}); const term=new Terminal({cols:60,rows:12});term.open(document.getElementById('term'));let sent=[];term.onData(data=>sent.push(data));const fail=error=>audit({stage:'error',msg:String(error)});const copy=async text=>audit({stage:'native-copy',text});const app=trackAppSelection(term,document.getElementById('term'));keepTerminalClipboard(term,copy,fail);term.attachCustomKeyEventHandler(e=>terminalShortcut(e,data=>term.input(data,true),{selection:()=>term.getSelection(),copy,fail,appSelection:app.active,appCopied:app.clear}));document.addEventListener('copy',e=>{e.preventDefault();audit({stage:'error',msg:'Copy must not depend on native WebKit copy events'})});term.write('COPY-MARKER',()=>{term.select(0,0,11);term.focus();setTimeout(()=>audit({stage:'ready'}),300)});</script>"
view.load_html(html, ROOT.as_uri()+'/')
GLib.timeout_add_seconds(15, Gtk.main_quit)
Gtk.main()
window.destroy()
assert not errors, errors
assert completed, 'WebKit shortcut test timed out'
print('PASS: explicit WebKit Ctrl+C copies without browser copy events; Ctrl+V uses bracketed paste; empty copy sends nothing; Ctrl+Shift+C sends one ETX; a real TUI mouse drag makes Ctrl+C hand over exactly one ETX; OSC 52 writes reach the GTK clipboard and queries go unanswered')
