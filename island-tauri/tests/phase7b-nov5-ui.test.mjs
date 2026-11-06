// Nov 5 UI: settings search / reset key sets / shortcut + font validation / monitor checklist /
// export-import, album colour pick + contrast, global motion speed, live xterm options.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const model = load('ui/views/settings-model.ts');
const album = load('ui/island/tokens/album.ts');
const motion = load('ui/island/motion-profile.ts');
const term = load('ui/views/terminal-options.ts');
const { SCHEMES } = load('ui/island/tokens/schemes.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const settingsKeys = () => [...src('ui/ipc/bindings.ts').match(/export type Settings = \{([\s\S]*?)\n\};/)[1]
  .replace(/\/\*\*[\s\S]*?\*\//g, '').matchAll(/^\s*(\w+)\??:/gm)].map(m => m[1]);
const ids = groups => groups.map(g => [g.card.id, g.rows.map(r => r.id)]);

test('search filters rows across tabs by label + hint, case/accent-insensitively, grouped by card', () => {
  const s = { translucent: true, companionEnabled: true, showNotchOnEveryMonitor: false, reduceMotion: false };
  assert.deepEqual(ids(model.searchRows('CURSOR', s)), [['text', ['terminalCursorStyle', 'terminalCursorBlink']]]);
  assert.deepEqual(ids(model.searchRows('game', s)), [['behavior', ['gameModeAuto', 'gameMode']]]);
  // Hint text matches too ("KDE syntax" lives only in the shortcut hint); every word must match.
  assert.deepEqual(ids(model.searchRows('kde syntax', s)), [['behavior', ['openShortcut']]]);
  assert.deepEqual(model.searchRows('kde banana', s), []);
  assert.deepEqual(model.searchRows('   ', s), [], 'blank query = no search');
  assert.deepEqual(ids(model.searchRows('móñitor', s)).map(([card]) => card), ['apps', 'notch'], 'accents folded; Hardware monitor + notch rows');
  // Rows hidden by their condition are not findable.
  assert.deepEqual(ids(model.searchRows('glass', { ...s, translucent: false })), [['surface', ['translucent']]], '"Frosted glass" hint');
  assert.deepEqual(ids(model.searchRows('glass', s)), [['surface', ['translucent', 'glassStrength']]]);
  assert.ok(!ids(model.searchRows('monitors', s)).flat(2).includes('notchMonitors'), 'checklist only with every-monitor on');
  assert.ok(ids(model.searchRows('monitors', { ...s, showNotchOnEveryMonitor: true })).flat(2).includes('notchMonitors'));
  // Page order: groups follow SETTINGS_CARDS order regardless of match position.
  const order = model.SETTINGS_CARDS.map(c => c.id), hit = model.searchRows('a', s).map(g => g.card.id);
  assert.deepEqual(hit, order.filter(id => hit.includes(id)));
});

test('shownRows is the active tab without a query and the search hits (any tab) with one', () => {
  const s = { translucent: false };
  const general = model.shownRows('general', '', s);
  assert.ok(general.has('gameMode') && general.has('openShortcut') && !general.has('reduceMotion'));
  assert.ok(!model.shownRows('appearance', '', s).has('glassStrength'));
  const hits = model.shownRows('layout', 'scrollback', s);
  assert.deepEqual([...hits], ['terminalScrollback']);
});

test('reset key sets: every card resets exactly its rows\' keys, keys are real settings, Nov 5 keys all reachable', () => {
  const keys = new Set(settingsKeys());
  assert.ok(keys.has('animationSpeed') && keys.has('moduleRegistry'));
  const owner = new Map();
  for (const card of model.SETTINGS_CARDS) for (const key of model.cardKeys(card.id)) {
    assert.ok(keys.has(key), `${card.id}: ${key} is not a Settings key`);
    assert.ok(!owner.has(key), `${key} is reset by both ${owner.get(key)} and ${card.id}`);
    owner.set(key, card.id);
  }
  assert.ok(!owner.has('moduleRegistry'), 'apps registry is never reset from Settings');
  assert.deepEqual(model.cardKeys('motion'), ['reduceMotion', 'animationSpeed']);
  assert.deepEqual(model.cardKeys('behavior'), ['swipeNavigation', 'stealth', 'clickSounds', 'gameModeAuto', 'gameMode', 'openShortcut']);
  for (const key of ['notchWaitingPulse', 'notchHoverPeek', 'companionReactive', 'notchMonitors', 'companionMonitor', 'showNotchOnEveryMonitor', 'companionCharacter'])
    assert.equal(owner.get(key), 'notch', key);
  for (const key of ['terminalFontFamily', 'terminalScrollback', 'terminalCursorStyle', 'terminalCursorBlink', 'terminalFontSize'])
    assert.equal(owner.get(key), 'text', key);
  for (const card of ['backup', 'terminalCommand', 'layoutEditor']) assert.equal(model.cardSpec(card).reset, false, card);
  const defaults = Object.fromEntries([...keys].map(k => [k, `default-${k}`]));
  assert.deepEqual(model.resetPatch('motion', defaults), { reduceMotion: 'default-reduceMotion', animationSpeed: 'default-animationSpeed' });
  assert.equal(model.cardSpec('scheme').resetsAppearance, true);
});

test('settings_defaults is a registered, permitted command and Reset reads defaults from it', () => {
  const bindings = src('ui/ipc/bindings.ts');
  for (const command of ['settings_defaults', 'display_outputs']) {
    assert.match(bindings, new RegExp(`__TAURI_INVOKE\\("${command}"`));
    assert.match(src('src-tauri/build.rs'), new RegExp(`"${command}"`));
    assert.match(src('src-tauri/permissions/island.toml'), new RegExp(`"allow-${command.replace('_', '-')}"`));
  }
  const view = src('ui/views/SettingsView.tsx');
  assert.match(view, /await settingsDefaults\(\)/);
  assert.doesNotMatch(view, /window\.confirm|<dialog/, 'reset confirms inline');
});

test('open shortcut and terminal font validation mirror the Rust rules', () => {
  for (const ok of ['Meta+`', 'Ctrl+Alt+Space', 'Meta+Shift+H', 'F12', 'Ctrl++', '']) assert.equal(model.shortcutError(ok), null, ok);
  for (const bad of ['Foo+X', 'Ctrl+', '+A', 'a', 'Ctrl+Ctrl+A', 'Meta,X', 'Meta+Shift', 'x'.repeat(65), 'Meta+\tX'])
    assert.ok(model.shortcutError(bad), bad);
  assert.equal(model.fontFamilyError('JetBrains Mono, Noto Color Emoji'), null);
  for (const bad of ['"x"', "a'b", 'a;b', 'a{', 'a}', '<b>', 'a\\b', 'x'.repeat(129)]) assert.ok(model.fontFamilyError(bad), bad);
});

test('monitor checklist: [] means all; unticking keeps the rest; ticking the last collapses back to []', () => {
  const outputs = ['DP-1', 'HDMI-A-1', 'eDP-1'];
  assert.deepEqual(model.toggleMonitor([], outputs, 'eDP-1', false), ['DP-1', 'HDMI-A-1']);
  assert.deepEqual(model.toggleMonitor(['DP-1', 'HDMI-A-1'], outputs, 'eDP-1', true), []);
  assert.deepEqual(model.toggleMonitor(['DP-1'], outputs, 'HDMI-A-1', true), ['DP-1', 'HDMI-A-1']);
  assert.deepEqual(model.toggleMonitor(['DP-1'], outputs, 'DP-1', true), ['DP-1'], 'no duplicates');
  assert.deepEqual(model.monitorChoices(['DP-9'], ['DP-1']), [
    { name: 'DP-1', checked: false, connected: true }, { name: 'DP-9', checked: true, connected: false }]);
  assert.ok(model.monitorChoices([], outputs).every(c => c.checked));
});

test('export omits the app registry; import yields one atomic patch and validates the shape', () => {
  const settings = { moduleRegistry: { modules: [] }, animationSpeed: 150, openShortcut: 'Meta+`' };
  const text = model.exportSettings(settings, 'album');
  const parsed = JSON.parse(text);
  assert.equal(parsed.format, 'monolith-settings'); assert.equal(parsed.appearance, 'album');
  assert.ok(!('moduleRegistry' in parsed.settings));
  const schemes = SCHEMES.map(s => s.id);
  assert.deepEqual(model.parseImport(text, schemes), { patch: { animationSpeed: 150, openShortcut: 'Meta+`' }, appearance: 'album' });
  assert.deepEqual(model.parseImport('{"dnd":true,"moduleRegistry":{}}', schemes), { patch: { dnd: true }, appearance: null }, 'bare object accepted');
  assert.throws(() => model.parseImport('{nope', schemes), /Not valid JSON/);
  assert.throws(() => model.parseImport('[1]', schemes), /JSON object/);
  assert.throws(() => model.parseImport('{"settings":[]}', schemes), /must be an object/);
  assert.throws(() => model.parseImport('{"settings":{}}', schemes), /No settings/);
  assert.throws(() => model.parseImport('{"format":"monolith-settings","appearance":"plaid","settings":{"dnd":true}}', schemes), /Unknown colour scheme/);
  // Secrets: nothing in Settings is a credential; guard against one being added silently.
  assert.doesNotMatch(settingsKeys().join(' '), /token|secret|password|apiKey/i);
});

const pixels = (...runs) => { const out = []; for (const [n, r, g, b, a = 255] of runs) for (let i = 0; i < n; i++) out.push(r, g, b, a); return Uint8ClampedArray.from(out); };
test('album colour: vivid dominant hue wins over grey, greyscale/transparent art falls back', () => {
  const red = album.pickAlbumColour(pixels([400, 128, 128, 128], [176, 220, 30, 40]));
  const [r, g, b] = [1, 3, 5].map(i => parseInt(red.slice(i, i + 2), 16));
  assert.ok(r > 180 && g < 80 && b < 80, `expected red, got ${red}`);
  // Two hues: the heavier saturated area wins.
  const blue = album.pickAlbumColour(pixels([300, 20, 60, 230], [100, 230, 200, 20]));
  assert.ok(parseInt(blue.slice(5, 7), 16) > 150, blue);
  assert.equal(album.pickAlbumColour(pixels([576, 90, 90, 90])), null, 'greyscale');
  assert.equal(album.pickAlbumColour(pixels([576, 255, 0, 0, 0])), null, 'fully transparent');
  assert.equal(album.pickAlbumColour(pixels([576, 2, 0, 1])), null, 'black');
  assert.equal(album.pickAlbumColour([]), null);
});

test('album colour keeps WCAG >= 3:1 against the surface and builds a full palette', () => {
  assert.equal(Math.round(album.contrastRatio('#000000', '#ffffff')), 21);
  assert.equal(album.contrastRatio('#123456', '#123456'), 1);
  for (const seed of ['#1a1a80', '#300000', '#ff0000', '#00ff88', '#fafa00', '#4b0082', '#808000']) {
    const p = album.albumPalette(seed);
    for (const [role, value] of Object.entries(p)) assert.match(value, /^#[0-9a-f]{6}$/, `${seed} ${role}`);
    assert.ok(album.contrastRatio(p.accent, p.surface) >= album.ALBUM_MIN_CONTRAST, `${seed} → ${p.accent} on ${p.surface}`);
    assert.ok(album.relativeLuminance(p.background) < .05, 'stays a dark island');
  }
  assert.equal(album.ensureContrast('#80ff36', '#121c15'), '#80ff36', 'compliant accents are untouched');
  const lifted = album.ensureContrast('#200040', '#101010');
  assert.ok(album.contrastRatio(lifted, '#101010') >= 3 && lifted !== '#200040');
  assert.ok(album.contrastRatio(album.ensureContrast('#ffff80', '#f4f4f4'), '#f4f4f4') >= 3, 'darkens on light surfaces');
});

test('Album is a scheme; sampling is once per artwork via a 24x24 canvas and the swap crossfades in CSS', () => {
  const scheme = SCHEMES.find(s => s.id === 'album');
  assert.ok(scheme && scheme.palette === null, 'runtime palette, Skin fallback');
  assert.equal(album.ALBUM_SAMPLE, 24); assert.equal(album.ALBUM_FADE_MS, 600);
  const runtime = src('ui/island/album-scheme.ts');
  assert.match(runtime, /sampled\?\.art === art/, 'memoized per artwork');
  assert.doesNotMatch(runtime, /requestAnimationFrame|setInterval/, 'no per-frame work');
  const css = src('ui/island/polish.css');
  assert.match(css, /@property --album-accent \{ syntax: '<color>'/);
  assert.match(css, /html\[data-theme='album'\] \{[^}]*transition-duration: var\(--album-fade/);
  assert.match(src('ui/island/Island.tsx'), /--album-fade', `\$\{settings\.reduceMotion \? 0 : ALBUM_FADE_MS\}ms`/);
});

test('animation speed scales springs centrally; withdraw/blur contracts stay fixed', () => {
  const size = (w, h) => ({ width: w, height: h });
  const base = motion.arrivalTime(size(144, 32), size(720, 460));
  motion.setMotionSpeed(200);
  try {
    const fast = motion.arrivalTime(size(144, 32), size(720, 460));
    assert.ok(Math.abs(fast - base / 2) < .02, `${fast} vs ${base}/2`);
    assert.equal(motion.scaled(.45), .225);
    assert.deepEqual(motion.uiSpring(.3, .15), { type: 'spring', visualDuration: .15, bounce: .15 });
    assert.equal(motion.WITHDRAW_SECONDS, .16);
    motion.setMotionSpeed(10); assert.equal(motion.motionSpeed(), .5, 'clamped to 50 %');
    motion.setMotionSpeed(NaN); assert.equal(motion.motionSpeed(), 1);
  } finally { motion.setMotionSpeed(100); }
  assert.equal(motion.arrivalTime(size(144, 32), size(720, 460)), base);
  const island = src('ui/island/Island.tsx');
  assert.match(island, /<MotionConfig reducedMotion=\{settings\.reduceMotion \? 'always' : 'user'\}/);
  assert.match(island, /setMotionSpeed\(settings\.animationSpeed/);
  assert.match(src('ui/island/IslandSurface.tsx'), /settings\.reduceMotion === true/);
});

test('terminal options map settings to xterm and apply to the live instance without recreating it', () => {
  assert.deepEqual(term.terminalOptions({ fontFamily: '', scrollback: 5000, cursorStyle: 'block', cursorBlink: true }),
    { fontFamily: '"DejaVu Sans Mono", monospace', scrollback: 5000, cursorStyle: 'block', cursorBlink: true });
  assert.equal(term.terminalFontFamily('JetBrains Mono'), 'JetBrains Mono, monospace');
  assert.equal(term.terminalFontFamily('Iosevka, monospace'), 'Iosevka, monospace');
  assert.equal(term.terminalFontFamily('evil"; }'), term.DEFAULT_TERMINAL_FONT);
  assert.equal(term.terminalOptions({ fontFamily: '', scrollback: 9e9, cursorStyle: 'x', cursorBlink: false }).scrollback, 100000);
  const options = { fontFamily: term.DEFAULT_TERMINAL_FONT, scrollback: 5000, cursorStyle: 'block', cursorBlink: true }, writes = [];
  const live = new Proxy(options, { set(t, k, v) { writes.push(k); t[k] = v; return true; } });
  assert.equal(term.applyTerminalOptions(live, { fontFamily: '', scrollback: 5000, cursorStyle: 'bar', cursorBlink: false }), false);
  assert.deepEqual(writes, ['cursorStyle', 'cursorBlink'], 'only changed options are written');
  assert.equal(term.applyTerminalOptions(live, { fontFamily: 'Hack', scrollback: 5000, cursorStyle: 'bar', cursorBlink: false }), true, 'font change refits');
  const view = src('ui/views/TerminalView.tsx');
  assert.equal(view.match(/new Terminal\(/g).length, 1);
  assert.match(view, /applyTerminalOptions\(term\.current\.options/);
  assert.match(view, /\}, \[fontFamily, scrollback, cursorStyle, cursorBlink\]\);/);
});

test('terminal measures emoji like the Hermes TUI (Unicode 11 widths, no ghost columns)', async () => {
  const view = src('ui/views/TerminalView.tsx');
  assert.match(view, /loadAddon\(new Unicode11Addon\(\)\); terminal\.unicode\.activeVersion = '11';/);
  assert.match(view, /allowProposedApi: true/);
  const pkg = JSON.parse(src('package.json'));
  assert.equal(pkg.dependencies['@xterm/addon-unicode11'], '0.9.0', 'pinned to the xterm 6.0.0 release');
});
