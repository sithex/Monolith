#!/usr/bin/env python3
"""Launcher CSS in isolated WebKitGTK; no app, IPC, or live desktop state."""
import json
import sys
from pathlib import Path
import gi

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2, GLib

ROOT = Path(__file__).resolve().parents[1]
links = ''.join('<link rel="stylesheet" href="' + (ROOT / path).as_uri() + '">' for path in [
    'ui/island/island.css', 'ui/island/polish.css', 'ui/island/motion-launcher.css',
])
button = '<button class="app-tile"><svg viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg><span>System monitor</span></button>'
tiles = ''.join('<div class="tile-wrap"><div class="tile-face" data-wiggle="true" style="--wiggle-period:.24s;--wiggle-delay:-.071s">' + button + '</div></div>' for _ in range(18))
reference = ''.join('<div class="tile-wrap">' + button + '</div>' for _ in range(18))
script = r"""
(async()=>{
 const wait=ms=>new Promise(r=>setTimeout(r,ms));
 const root=document.querySelector('.island'), launcher=document.querySelector('.launcher');
 const face=launcher.querySelector('.tile-face'), style=()=>getComputedStyle(face);
 const check=(condition,message)=>{if(!condition)throw Error(message)};
 const results=[];
 // Compare actual settled tile geometry before/after adding the rotation wrapper.
 face.closest('.launcher').dataset.reducedMotion='true';
 for(const width of [300,520,760,1100,1800]) {
  root.style.width=width+'px'; await wait(30);
  const grids=[...document.querySelectorAll('.app-grid')];
  const reference=[...document.querySelectorAll('.reference .app-tile')].map(e=>e.getBoundingClientRect());
  for(const grid of grids) {
   const boxes=[...grid.querySelectorAll('.app-tile')].map(e=>e.getBoundingClientRect());
   check(boxes.every(r=>r.width<=110.5&&r.height<=104),'tile cap at '+width);
   check(boxes.every((r,i)=>Math.abs(r.width-reference[i].width)<.01&&Math.abs(r.height-reference[i].height)<.01),'wrapper changes tile size at '+width);
   const icon=grid.querySelector('svg').getBoundingClientRect();
   check(icon.width<=28.5,'icon cap at '+width);
   results.push({width,hidden:grid.classList.contains('hidden-grid'),tileWidth:boxes[0].width,tileHeight:boxes[0].height,iconWidth:icon.width});
  }
 }
 launcher.dataset.reducedMotion='false'; launcher.dataset.motionActive='true';
 await wait(40); const first=style().transform;
 await wait(63); check(style().transform!==first,'visible CSS wiggle did not move');
 check(Math.abs(parseFloat(style().animationDuration)-.24)<.001,'wrong wiggle period');
 launcher.dataset.motionActive='false'; await wait(30); const paused=style().transform;
 await wait(80); check(style().transform===paused,'hidden wiggle kept moving');
 check(style().animationPlayState==='paused','hidden animation not paused');
 launcher.dataset.motionActive='true'; launcher.dataset.reducedMotion='true'; await wait(30);
 check(style().transform==='none','reduced motion rotates');
 check(getComputedStyle(face.querySelector('button')).outlineStyle==='dashed','missing static edit cue');
 launcher.dataset.reducedMotion='false'; document.documentElement.dataset.navVia='key'; await wait(30);
 check(style().transform==='none','keyboard wiggle');
 delete document.documentElement.dataset.navVia;
 root.style.setProperty('--motion-speed','2'); await wait(30);
 check(Math.abs(parseFloat(style().animationDuration)-.12)<.001,'wiggle ignores animation speed');
 window.webkit.messageHandlers.audit.postMessage(JSON.stringify({measurements:results,checks:['size parity','visible motion','hidden pause','static reduced cue','keyboard instant','speed scaling']}));
})().catch(e=>{document.title='ERROR:'+String(e)});
"""
html = '<!doctype html><html><head>' + links + '</head><body><section class="island" data-state="large" style="position:relative;left:0;top:0;height:900px;transform:none;opacity:1"><div class="app-pane"><div class="launcher" data-editing="true" data-motion-active="false"><div class="app-grid" data-editing="true">' + tiles + '</div><div class="app-grid hidden-grid" data-editing="true">' + tiles + '</div></div><div class="reference app-grid" data-editing="true">' + reference + '</div></div></section><script>addEventListener("load",()=>{' + script + '});</script></body></html>'
manager = WebKit2.UserContentManager()
manager.register_script_message_handler('audit')
view = WebKit2.WebView.new_with_user_content_manager(manager)
window = Gtk.Window()
result = []
errors = []

def received(_manager, message):
    result.append(json.loads(message.get_js_value().to_string()))
    Gtk.main_quit()

def title_changed(widget, _):
    title = widget.get_title() or ''
    if title.startswith('ERROR:'):
        errors.append(title)
        Gtk.main_quit()

manager.connect('script-message-received::audit', received)
view.connect('notify::title', title_changed)
window.add(view)
window.set_default_size(2000, 1400)
window.show_all()
view.load_html(html, ROOT.as_uri() + '/')
GLib.timeout_add_seconds(15, Gtk.main_quit)
Gtk.main()
window.destroy()
if errors:
    print('\n'.join(errors), file=sys.stderr)
assert result, 'WebKit did not return successful motion/geometry measurements'
print(json.dumps(result[0], indent=2))
print('PASS: wrapper size parity at five widths; CSS motion, pause, reduced/keyboard and speed checks')
