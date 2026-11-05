// Nov 2 (second batch): colour schemes (Green + Custom), Hermes theme sync payload,
// terminal default-colour ownership, emerging side-button blur, notch clock, glass strength.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { SCHEMES, GREEN, customPalette, schemePalette, mixHex } = load('ui/island/tokens/schemes.ts');
const { themePayload } = load('ui/island/hermes-theme.ts');
const { keepIslandColours } = load('ui/views/terminal-colours.ts');
const { geometryReport, sideBackFor } = load('ui/island/geometry.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const HEX = /^#[0-9a-f]{6}$/;

test('Green is the frozen necron look and every scheme id is a Rust Appearance', () => {
  assert.deepEqual(GREEN, { background: '#080d0a', surface: '#121c15', border: '#3d6243', dim: '#8a9c8d', text: '#d8e2d7', primary: '#e7eee4', accent: '#80ff36', error: '#ff7272' });
  const ids = SCHEMES.map(s => s.id);
  assert.equal(new Set(ids).size, ids.length);
  // Doc comments may contain ';' (the Album variant's does), so drop them before matching.
  const union = src('ui/ipc/bindings.ts').replace(/\/\*\*[\s\S]*?\*\//g, '').match(/export type Appearance = ([^;]+);/)[1];
  for (const id of ids) assert.match(union, new RegExp(`"${id}"`), `${id} must exist in the Rust enum`);
  assert.equal(schemePalette('skin', {}), null, 'Hermes skin follows the live skin');
  assert.equal(SCHEMES[0].id, 'skin', 'following the Hermes skin stays the default (first) scheme');
  assert.equal(SCHEMES.find(s => s.id === 'green').name, 'Green');
  assert.deepEqual(schemePalette('green', {}), GREEN);
  assert.ok(schemePalette('neutral', {}), 'neutral now drives xterm and theme sync too');
});

test('Custom scheme derives every role from three seeds and falls back to green', () => {
  const empty = customPalette({});
  assert.equal(empty.background, GREEN.background); assert.equal(empty.accent, GREEN.accent); assert.equal(empty.text, GREEN.text);
  const mine = customPalette({ customAccent: '#FF0080', customBackground: '#101020', customText: '#eeeeff' });
  assert.equal(mine.accent, '#ff0080'); assert.equal(mine.background, '#101020'); assert.equal(mine.text, '#eeeeff');
  for (const [role, value] of Object.entries(mine)) assert.match(value, HEX, role);
  assert.notEqual(mine.surface, mine.background); assert.notEqual(mine.dim, mine.text);
  assert.equal(customPalette({ customAccent: 'red' }).accent, GREEN.accent, 'invalid seeds are ignored');
  const light = customPalette({ customBackground: '#f4f4f4' });
  assert.ok(parseInt(light.text.slice(1, 3), 16) < 0x40, 'a light background gets dark text by default');
  assert.equal(schemePalette('custom', { customAccent: '#123456' }).accent, '#123456');
  assert.equal(mixHex('#000000', '#ffffff', .5), '#808080');
});

test('theme sync sends exactly the 8 skin roles, lowercased, or nothing', () => {
  const roles = { ...GREEN, accent: '#80FF36' };
  assert.deepEqual(themePayload(roles), { name: 'ripple-island', ...GREEN });
  assert.equal(themePayload({ ...GREEN, dim: 'var(--x)' }), null);
  assert.equal(themePayload({ ...GREEN, error: undefined }), null);
  const hook = src('ui/island/hermes-theme.ts');
  assert.match(hook, /setTimeout\(/, 'colour drags are debounced into one skin write');
  assert.match(hook, /if \(!enabled && !turningOff\) return;/, 'nothing runs at startup while sync is off');
});

test('the island owns the terminal default colours: OSC 10/11 sets and 110/111 resets are swallowed, queries answered', () => {
  const handlers = new Map();
  const parser = { registerOscHandler: (id, fn) => { handlers.set(id, fn); return { dispose: () => handlers.delete(id) }; } };
  const disposables = keepIslandColours({ parser });
  assert.deepEqual([...handlers.keys()].sort((a, b) => a - b), [10, 11, 110, 111]);
  assert.equal(handlers.get(11)('#080d0a'), true, 'skin background set is swallowed (glass stays visible)');
  assert.equal(handlers.get(10)('#d8e2d7'), true);
  assert.equal(handlers.get(11)('?'), false, 'background query falls through to xterm');
  assert.equal(handlers.get(10)('?'), false);
  assert.equal(handlers.get(111)(''), true); assert.equal(handlers.get(110)(''), true);
  disposables.forEach(d => d.dispose()); assert.equal(handlers.size, 0);
  assert.match(src('ui/views/TerminalView.tsx'), /const osc = keepIslandColours\(terminal\);/, 'the live terminal installs it');
});

test('emerging side circles report their drawn face (blur from the first frame); settled ones their layout box', () => {
  const face = { getBoundingClientRect: () => ({ x: 30, y: 40, width: 20, height: 20 }) };
  const emerging = { dataset: { interactive: 'false' }, getClientRects: () => [{}], querySelector: () => face };
  const settled = { dataset: { interactive: 'true' }, offsetLeft: 5, offsetTop: 6, offsetWidth: 44, offsetHeight: 44, getClientRects: () => [{}] };
  const outer = { ownerDocument: { querySelectorAll: () => [emerging, settled] }, getBoundingClientRect: () => ({ x: 100, y: 11, width: 600, height: 500 }) };
  globalThis.getComputedStyle = e => ({ visibility: 'visible', borderRadius: e === settled ? '22px' : '28px' });
  const rects = geometryReport(outer, { rev: 3, mode: 'large' }, true, 1).rects;
  assert.deepEqual(rects.slice(1), [{ x: 30, y: 40, width: 20, height: 20, radius: 10 }, { x: 5, y: 6, width: 44, height: 44, radius: 22 }]);
  const tiny = { ...emerging, querySelector: () => ({ getBoundingClientRect: () => ({ x: 0, y: 0, width: 1, height: 1 }) }) };
  outer.ownerDocument.querySelectorAll = () => [tiny];
  assert.equal(geometryReport(outer, { rev: 3, mode: 'large' }, true, 1).rects.length, 1, 'a not-yet-drawn circle is not blurred');
  assert.match(src('ui/island/SideControls.tsx'), /progress\.on\('change'/, 'each emergence frame re-reports the circles');
});

test('the island clock replicates the native notch clock in the notch box and never fades out on close', () => {
  const css = src('ui/island/island.css');
  assert.doesNotMatch(css, /\.island\[data-state='still'\] \.topline \{ opacity: 0/, 'hiding it on close was the blink');
  assert.match(css, /\.topline\[data-notch-clock='true'\] \.clock \{ position: absolute; top: calc\(-1 \* var\(--bw\) - 2px\); left: 50%; width: var\(--native-pill-width\); height: var\(--native-pill-height\)/);
  assert.match(css, /\.clock span \{ display: block; position: relative; top: 1px; height: 14px; font-size: 12px; font-weight: 700; line-height: 14px; \}/, 'PanelPill.qml: 12 px bold, 14 px tall');
  assert.match(css, /\.clock small \{ display: block; height: 12px; margin-top: 0; font-size: 10px; font-weight: 600;/, 'PanelPill.qml: 10 px demibold, spacing -1');
  const qml = readFileSync(new URL('../../island/plasma-anchor/contents/ui/PanelPill.qml', import.meta.url), 'utf8');
  assert.match(qml, /twoLines: pill\.showDate \|\| pill\.timerShown/); assert.match(qml, /font\.pixelSize: parent\.twoLines \? 12 : 14/); assert.match(qml, /font\.pixelSize: 10/); assert.match(qml, /spacing: -1/);
});

test('glass strength drives the body and side-button transparency', () => {
  const css = src('ui/island/surface.css');
  assert.match(css, /--glass-cut: calc\(var\(--glass-open, 1\) \* var\(--glass-strength\) \* 100%\)/);
  assert.match(css, /\.live-side-button \{ background: color-mix\(in srgb, var\(--ui-background\) calc\(100% - var\(--glass-strength\) \* 100%\), transparent\); \}/);
  assert.match(src('ui/island/Island.tsx'), /setProperty\('--glass-strength'/);
});

test('the native notch follows the scheme (hover tint = scheme accent) and the replica clock uses the same text', () => {
  const island = src('ui/island/Island.tsx');
  assert.match(island, /notchSchemeSet\(JSON\.parse\(notchScheme\)\)/);
  assert.match(island, /setProperty\('--notch-text', \(scheme \?\? palette\)\.text\)/);
  assert.match(src('src-tauri/src/integrations/plasma/runtime.rs'), /let palette = scheme\.or\(skin\);/);
});

test('side-button hover is explicit state cleared by the real X11 leave, not a sticky :hover', () => {
  const side = src('ui/island/SideControls.tsx'), css = src('ui/island/surface.css');
  assert.doesNotMatch(side, /whileHover=/);
  assert.match(side, /addEventListener\('island-pointer-left', clear\)/);
  assert.doesNotMatch(css, /\.live-side-button:hover/);
  assert.match(css, /\.live-side-button\[data-hover='true'\]/);
  assert.match(src('ui/island/workshop.css'), /button:hover:not\(\.live-side-button\)/);
  assert.match(src('src-tauri/src/window/mod.rs'), /connect_leave_notify_event[\s\S]*island-pointer-left/);
});

test('Now Playing transport uses drawn glyphs with a solid play/pause disc', () => {
  const np = src('ui/island/components/NowPlaying.tsx'), css = src('ui/island/media.css');
  assert.doesNotMatch(np, /'Ⅱ'|'▶'|\|‹|›\|/);
  assert.match(np, /<TransportGlyph kind="play" \/><TransportGlyph kind="pause" \/>/);
  assert.match(css, /\.island \.now-playing \.transport \.transport-main \{[^}]*background: var\(--ui-text\)/, 'beats the transparent transport button rule');
});

test('the solid top strip eases into the glass instead of a hard 42 px edge', () => {
  const css = src('ui/island/surface.css');
  assert.doesNotMatch(css, /var\(--ui-background\) 42px,/);
  assert.ok((css.match(/var\(--glass-cut\) \* \.\d+\), transparent\) \d+px/g) ?? []).length >= 8, 'multi-stop eased ramp');
});

test('a page side button becomes Back while its page is showing, and returns to where you were', () => {
  const back = { view: 'terminal', label: 'Terminal', go() {} };
  assert.equal(sideBackFor({ type: 'appGrid' }, 'launcher', back), back, 'Apps on the app grid = Back');
  assert.equal(sideBackFor({ type: 'settings' }, 'settings', { ...back, view: 'music' }).view, 'music', 'back target is the page you came from');
  assert.equal(sideBackFor({ type: 'appGrid' }, 'settings', back), null, 'another page: normal icon');
  assert.equal(sideBackFor({ type: 'openApp', view: 'terminal' }, 'terminal', back), null, 'nowhere else to go: no Back');
  assert.equal(sideBackFor({ type: 'playPause' }, 'music', { ...back, view: 'launcher' }), null, 'quick actions never turn into Back');
  assert.equal(sideBackFor({ type: 'appGrid' }, undefined, back), null);
  const side = src('ui/island/SideControls.tsx');
  assert.match(side, /if \(returning\) back!\.go\(\)/, 'the tap runs the island back history, not a fresh navigation');
  assert.match(side, /returning \? 'back' : actionIcon/);
  // The spinning wrapper must fill the button: the icon is sized in % of its container.
  assert.match(src('ui/island/polish.css'), /\.live-side-button \.side-icon \{[^}]*width: 100%; height: 100%/);
  assert.match(src('ui/island/Island.tsx'), /<SideControls[^>]*back=\{\{ view: previous, label: viewLabels\[previous\], go: goBack \}\}/);
});
