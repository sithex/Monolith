#!/usr/bin/env python3
"""Real WebKitGTK CSS regression; isolated Xvfb, no IPC or live island state.
Run: env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a /usr/bin/python3 tests/launcher_tile_geometry.py
"""
import json
import sys
from pathlib import Path
import gi

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2, GLib

ROOT = Path(__file__).resolve().parents[1]
CSS = (ROOT / 'ui/island/island.css').as_uri()
POLISH = (ROOT / 'ui/island/polish.css').as_uri()
# Layout assertions must not sample an in-flight hover/entrance animation.
STILL = '<style>*,*::before,*::after{animation:none!important;transition:none!important;}</style>'
TILES = ''.join('<div class="tile-wrap"><button class="app-tile"><svg viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg><span>System monitor</span></button></div>' for _ in range(18))
SCRIPT = r"""
(async()=>{
 const rows=[];
 for(const width of [300,520,760,1100,1800]) {
  const island=document.querySelector('.island'); island.style.width=width+'px';
  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
  for(const grid of document.querySelectorAll('.app-grid')) {
   const rect=grid.getBoundingClientRect(),tiles=[...grid.querySelectorAll('.app-tile')];
   const boxes=tiles.map(e=>e.getBoundingClientRect());
   rows.push({width,hidden:grid.classList.contains('hidden-grid'),tileWidth:Math.max(...boxes.map(r=>r.width)),
    tileHeight:Math.max(...boxes.map(r=>r.height)),icon:tiles[0].querySelector('svg').getBoundingClientRect().width,
    font:parseFloat(getComputedStyle(tiles[0]).fontSize),columns:boxes.filter(r=>Math.abs(r.top-boxes[0].top)<1).length,
    fits:boxes.every(r=>r.left>=rect.left-.5&&r.right<=rect.right+.5)});
  }
 }
 window.webkit.messageHandlers.audit.postMessage(JSON.stringify(rows));
})().catch(e=>document.title='ERROR:'+String(e));
"""
html = '<!doctype html><html><head><link rel="stylesheet" href="'+CSS+'"><link rel="stylesheet" href="'+POLISH+'">'+STILL+'</head><body><section class="island" data-state="large" style="position:relative;left:0;top:0;height:900px;transform:none;opacity:1"><div class="app-pane"><div class="launcher"><div class="app-grid">'+TILES+'</div><div class="app-grid hidden-grid">'+TILES+'</div></div></div></section><script>window.addEventListener("load",()=>{'+SCRIPT+'});</script></body></html>'
window = Gtk.Window()
manager = WebKit2.UserContentManager()
manager.register_script_message_handler('audit')
view = WebKit2.WebView.new_with_user_content_manager(manager)
result = []

def received(_manager, message):
    result.extend(json.loads(message.get_js_value().to_string()))
    Gtk.main_quit()

manager.connect('script-message-received::audit', received)

def title_changed(widget, _):
    title = widget.get_title() or ''
    if title.startswith('ERROR:'):
        print(title, file=sys.stderr); Gtk.main_quit()

view.connect('notify::title', title_changed)
window.add(view)
window.set_default_size(2000, 1400)
window.show_all()
view.load_html(html, ROOT.as_uri()+'/')
GLib.timeout_add_seconds(15, Gtk.main_quit)
Gtk.main()
window.destroy()
assert result, 'WebKit did not return layout measurements'
print(json.dumps(result, indent=2))
for row in result:
    assert row['tileWidth'] <= 110.5, f'Tile expands with island width: {row}'
    assert row['tileHeight'] <= 104, f'Tile height exceeds compact maximum: {row}'
    assert row['icon'] <= 28.5, f'Icon expands with island width: {row}'
    assert row['font'] <= 12, f'Label expands with island width: {row}'
    assert row['fits'], f'Tiles overflow narrow grid: {row}'
for hidden in [False, True]:
    group = [r for r in result if r['hidden'] == hidden]
    assert group[-1]['columns'] > group[0]['columns'], 'Wider grids must add columns rather than grow tiles'
print('PASS: shown/hidden tiles, icons and labels are capped at all tested island widths')
