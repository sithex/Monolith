#!/usr/bin/env python3
"""Isolated WebKit CSS check; no island process, sockets or desktop configuration.

Run with GDK_BACKEND=x11 under xvfb-run. The fixture uses the production stylesheets
and a private runtime directory so accessibility/portal activation cannot touch the desktop.
"""
import json
import os
from pathlib import Path
import tempfile
import sys

os_reduced = "--os-reduced" in sys.argv

runtime = tempfile.TemporaryDirectory(prefix="transient-motion-", dir=os.environ.get("TMPDIR"))
os.chmod(runtime.name, 0o700)
os.environ["XDG_RUNTIME_DIR"] = runtime.name
os.environ["NO_AT_BRIDGE"] = "1"
os.environ.pop("DBUS_SESSION_BUS_ADDRESS", None)

import gi  # type: ignore[import-not-found]  # noqa: E402

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # type: ignore[import-not-found]  # noqa: E402

if os_reduced:
    Gtk.Settings.get_default().set_property("gtk-enable-animations", False)

root = Path(__file__).resolve().parents[1]
css = "\n".join((root / path).read_text() for path in [
    "ui/island/surface.css", "ui/island/polish.css", "ui/island/command-palette.css",
    "ui/island/approval-notice.css", "ui/island/notifications/notification-cards.css",
    "ui/island/motion-transient.css", "ui/island/tokens/scheme-picker.css",
    "ui/views/settings/settings-controls.css",
])
html = f"""<!doctype html><html data-nav-via="pointer"><style>{css}</style>
<style>.island {{ position:relative; width:900px; height:850px; }}
.dropdown {{ width:220px; margin:60px; }} .dropdown-menu {{ width:200px; height:100px; }}
</style><body><div class="island">
<div class="dropdown" id="down"><button>Trigger</button><div class="dropdown-menu"></div></div>
<div class="dropdown" id="up" data-up="true"><button>Trigger</button><div class="dropdown-menu"></div></div>
<div class="dropdown font-picker" id="font" data-up="true"><button>Font</button><div class="dropdown-menu font-menu"></div></div>
<div class="command-results"><button aria-selected="true" id="row">Command</button></div>
<div class="approval-notice"><div class="approval-actions" id="choices" data-armed="false"><button class="approval-choice" id="choice">Once</button></div></div>
<div class="notification-timer"><span id="timer" style="animation-duration:5s;--remaining:.8"></span></div>
</div></body></html>"""
script = """(() => {
  const get = s => document.querySelector(s), root = document.documentElement;
  const style = s => getComputedStyle(get(s));
  const result = {osReduced:matchMedia('(prefers-reduced-motion: reduce)').matches};
  for (const id of ['down', 'up', 'font']) {
    const node = get('#' + id + ' .dropdown-menu');
    for (const a of node.getAnimations()) { a.pause(); a.currentTime = 0; }
    const s = getComputedStyle(node), matrix = new DOMMatrix(s.transform);
    result[id] = {name:s.animationName, duration:s.animationDuration, x:matrix.m11, y:matrix.m42,
      origin:s.transformOrigin, width:node.offsetWidth, height:node.offsetHeight};
  }
  result.dim = style('#choice').opacity;
  get('#choices').dataset.armed = 'true';
  for (const a of get('#choice').getAnimations()) a.finish();
  result.armed = style('#choice').opacity;
  root.dataset.navVia = 'key';
  result.keyTransition = style('#row').transitionDuration;
  result.keyAnimation = style('#up .dropdown-menu').animationName;
  root.dataset.navVia = 'pointer'; root.dataset.reduceMotion = 'true';
  result.reducedMenu = {name:style('#up .dropdown-menu').animationName, duration:style('#up .dropdown-menu').animationDuration};
  const timer = get('#timer'), countdown = timer.getAnimations()[0];
  countdown.pause(); countdown.currentTime = 1000;
  result.timerStart = new DOMMatrix(style('#timer').transform).m11;
  countdown.currentTime = 2000;
  result.timerLater = new DOMMatrix(style('#timer').transform).m11;
  result.timer = {duration:style('#timer').animationDuration, ease:style('#timer').animationTimingFunction};
  root.dataset.navVia = 'key';
  result.reducedKeyAnimation = style('#up .dropdown-menu').animationName;
  return JSON.stringify(result);
})()"""
window = Gtk.Window()
view = WebKit2.WebView()
window.add(view)
window.set_default_size(1000, 900)
result = {}


def evaluated(webview, task, *_):
    try:
        result.update(json.loads(webview.evaluate_javascript_finish(task).to_string()))
    except Exception as error:
        result["error"] = str(error)
    Gtk.main_quit()


def loaded(webview, event):
    if event == WebKit2.LoadEvent.FINISHED:
        webview.evaluate_javascript(script, -1, None, None, None, evaluated, None)


view.connect("load-changed", loaded)
window.show_all()
view.load_html(html, "file:///nonexistent-transient-fixture/")
GLib.timeout_add_seconds(20, lambda: (result.update(error="WebKit fixture timed out"), Gtk.main_quit(), False)[-1])
Gtk.main()
window.destroy()
assert "error" not in result, result
assert result["osReduced"] == os_reduced, result
for key, direction in [("down", -4), ("up", 4), ("font", 4)]:
    item = result[key]
    assert abs(item["y"] - (0 if os_reduced else direction)) < .01, item
    assert abs(item["x"] - (1 if os_reduced else .97)) < .001, item
    assert item["duration"] == ("0.12s" if os_reduced else "0.18s"), item
    assert item["name"] == ("transient-fade-in" if os_reduced else "transient-dropdown-in" if key == "down" else "transient-dropdown-up"), item
    ox, oy = [float(v.removesuffix("px")) for v in item["origin"].split()]
    assert abs(oy - (0 if key == "down" else item["height"])) < 1, item
    if key == "font":
        assert abs(ox - item["width"]) < 1, item
assert float(result["dim"]) == .55 and float(result["armed"]) == 1, result
assert all(float(v.strip().removesuffix("s")) == 0 for v in result["keyTransition"].split(",")), result
assert result["keyAnimation"] == result["reducedKeyAnimation"] == "none", result
assert result["reducedMenu"] == {"name": "transient-fade-in", "duration": "0.12s"}, result
assert result["timer"] == {"duration": "5s", "ease": "linear"}, result
assert 0 < result["timerLater"] < result["timerStart"] < .8, result
print(json.dumps(result, indent=2))
print("PASS: directions, trigger origins, keyboard instantness, arm brightness, reduced fade and linear countdown")
runtime.cleanup()
