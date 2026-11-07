// Nov 6 settings controls: shortcut recorder key mapping, font picker model, per-app rules model.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { load } from './phase7b-loader.mjs';
const keys = load('ui/views/settings/shortcut-keys.ts');
const fonts = load('ui/views/settings/font-list.ts');
const rules = load('ui/views/settings/app-rules.ts');
const model = load('ui/views/settings-model.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

const press = (key, extra = {}) => ({ key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...extra });
const text = (key, extra) => { const c = keys.chordFromKey(press(key, extra)); return c.kind === 'chord' ? c.text : c.kind; };

test('recorder: key press → KGlobalAccel text, in KDE modifier order', () => {
  assert.equal(text('`', { metaKey: true }), 'Meta+`');
  assert.equal(text(' ', { ctrlKey: true, altKey: true }), 'Ctrl+Alt+Space');
  assert.equal(text('h', { metaKey: true, shiftKey: true }), 'Meta+Shift+H');
  assert.equal(text('H', { metaKey: true, shiftKey: true }), 'Meta+Shift+H');
  assert.equal(text('t', { altKey: true, ctrlKey: true, metaKey: true, shiftKey: true }), 'Meta+Ctrl+Alt+Shift+T');
  assert.equal(text('F12'), 'F12');
  assert.equal(text('F5', { shiftKey: true, ctrlKey: true }), 'Ctrl+Shift+F5');
  assert.equal(text('Enter', { metaKey: true }), 'Meta+Return');
  assert.equal(text('ArrowLeft', { altKey: true }), 'Alt+Left');
  assert.equal(text('PageDown', { ctrlKey: true }), 'Ctrl+PgDown');
  assert.equal(text('Delete', { ctrlKey: true }), 'Ctrl+Del');
  assert.equal(text('Escape', { ctrlKey: true }), 'Ctrl+Esc');
  assert.equal(text('PrintScreen'), 'Print');
  assert.equal(text('+', { ctrlKey: true }), 'Ctrl++');
  assert.equal(text('-', { metaKey: true }), 'Meta+-');
});

test('recorder: shifted symbols are written without Shift (as kglobalshortcutsrc does)', () => {
  assert.equal(text('!', { metaKey: true, shiftKey: true }), 'Meta+!');
  assert.equal(text('~', { altKey: true, shiftKey: true }), 'Alt+~');
  assert.equal(text('?', { ctrlKey: true, shiftKey: true }), 'Ctrl+?');
});

test('recorder: dead / non-Latin keys fall back to the physical key', () => {
  assert.equal(text('Dead', { metaKey: true, code: 'Backquote' }), 'Meta+`');
  assert.equal(text('ф', { ctrlKey: true, code: 'KeyA' }), 'Ctrl+A');
  assert.equal(text('Unidentified', { altKey: true, code: 'Digit7' }), 'Alt+7');
  assert.equal(keys.chordFromKey(press('é', { metaKey: true })).kind, 'invalid', 'no physical code → refuse');
});

test('recorder: lone modifiers wait, Esc cancels, Backspace clears, bare letters and comma are refused', () => {
  for (const k of ['Meta', 'Control', 'Alt', 'Shift', 'AltGraph', 'CapsLock']) assert.equal(keys.chordFromKey(press(k)).kind, 'modifier', k);
  assert.equal(keys.chordFromKey(press('Control', { ctrlKey: true, metaKey: true })).held, 'Meta+Ctrl+…');
  assert.equal(keys.chordFromKey(press('Escape')).kind, 'cancel');
  assert.equal(keys.chordFromKey(press('Backspace')).kind, 'clear');
  assert.equal(keys.chordFromKey(press('Delete')).kind, 'clear');
  assert.equal(keys.chordFromKey(press('a')).kind, 'invalid');
  assert.equal(keys.chordFromKey(press('Enter')).kind, 'invalid');
  assert.equal(keys.chordFromKey(press('a', { shiftKey: true })).kind, 'invalid');
  assert.equal(keys.chordFromKey(press(',', { metaKey: true })).kind, 'invalid');
});

test('recorder: Super reported as OS / getModifierState counts as Meta', () => {
  assert.equal(text('k', { getModifierState: n => n === 'OS' }), 'Meta+K');
  assert.equal(text('k', { getModifierState: n => n === 'Super', ctrlKey: true }), 'Meta+Ctrl+K');
});

test('recorder: every key we can emit passes the settings validator (mirror of Rust keyseq::parse)', () => {
  const names = ['Escape', 'Tab', 'Backspace', 'Enter', 'Insert', 'Delete', 'Pause', 'PrintScreen', 'Home', 'End', 'ArrowLeft', 'ArrowUp', 'ArrowRight',
    'ArrowDown', 'PageUp', 'PageDown', ' ', 'ContextMenu', ...Array.from({ length: 35 }, (_, i) => `F${i + 1}`),
    ...'abcdefghijklmnopqrstuvwxyz0123456789`-=[]\\;\'./'.split('')];
  for (const key of names) for (const extra of [{ metaKey: true }, { ctrlKey: true, altKey: true }, { metaKey: true, shiftKey: true }]) {
    const c = keys.chordFromKey(press(key, extra));
    if (c.kind !== 'chord') continue;
    assert.equal(model.shortcutError(c.text), null, `${key} ${JSON.stringify(extra)} → ${c.text}`);
  }
  assert.equal(model.shortcutError('F36'), 'Unknown key "F36"');
  assert.equal(model.shortcutError('Meta+Foo'), 'Unknown key "Foo"');
  assert.equal(model.shortcutError('Meta+é') !== null, true);
  assert.equal(model.shortcutError('Hyper+X') !== null, true);
  assert.equal(model.shortcutError('Super+Win+X'), 'A modifier is repeated');
});

test('shortcut rows use the recorder, stay unbound by default and never preset a key', () => {
  const view = src('ui/views/SettingsView.tsx');
  assert.match(view, /<ShortcutRecorder label=\{label\} value=\{settings\[id\] \?\? ''\}/);
  assert.match(view, /onSave=\{value => saveSettings\(\{ \[id\]: value \}\)\}/);
  assert.match(view, /case 'openShortcut': case 'paletteShortcut':/);
  const rec = src('ui/views/settings/ShortcutRecorder.tsx');
  assert.doesNotMatch(rec, /useEffect\(\(\) => \{[^}]*save\(['"`][A-Za-z]/, 'no automatic save of a literal chord');
  assert.match(rec, /Type it/, 'typed fallback kept for chords KWin grabs');
});

// ── fonts ───────────────────────────────────────────────────────────────────────────────
const mk = n => Array.from({ length: n }, (_, i) => ({ family: `Family${String(i).padStart(4, '0')}${i % 5 === 0 ? 'Mono' : ''}`, monospace: i % 5 === 0 }));

test('font entries: Default first, monospace filter, every-word search, current value always listed', () => {
  const list = [{ family: 'DejaVu Sans', monospace: false }, { family: 'DejaVu Sans Mono', monospace: true }, { family: 'Fira Code', monospace: true }, { family: 'Inter', monospace: false }];
  assert.deepEqual(fonts.fontEntries(list, '', true, '').map(e => e.value), ['', 'DejaVu Sans Mono', 'Fira Code']);
  assert.deepEqual(fonts.fontEntries(list, '', false, '').map(e => e.value), ['', 'DejaVu Sans', 'DejaVu Sans Mono', 'Fira Code', 'Inter']);
  assert.deepEqual(fonts.fontEntries(list, 'sans mono', false, '').map(e => e.value), ['', 'DejaVu Sans Mono'], 'Default (DejaVu Sans Mono) matches too');
  assert.deepEqual(fonts.fontEntries(list, 'sans mono inter', false, '').map(e => e.value), [], 'every word must match');
  assert.deepEqual(fonts.fontEntries(list, 'FIRA', true, '').map(e => e.value), ['Fira Code']);
  assert.equal(fonts.fontEntries(list, 'default', true, '')[0].value, '', 'Default is searchable');
  assert.deepEqual(fonts.fontEntries(list, '', true, 'Inter').map(e => e.value), ['', 'DejaVu Sans Mono', 'Fira Code', 'Inter'], 'saved non-mono font stays visible');
  assert.deepEqual(fonts.fontEntries(list, '', true, 'Gone Font').map(e => e.value).slice(0, 2), ['', 'Gone Font'], 'saved font that is not installed stays visible');
  assert.deepEqual(fonts.fontEntries(list, 'zzz', true, ''), []);
});

test('font window: never more than the cap, follows the scroll position, covers the viewport', () => {
  const total = 1139;
  for (const scrollTop of [0, 31, 32, 5000, total * fonts.FONT_ROW_PX, total * fonts.FONT_ROW_PX * 2]) {
    const { start, end } = fonts.fontWindow(scrollTop, 256, total);
    assert.ok(end - start <= fonts.FONT_MAX_RENDERED && end - start > 0 && start >= 0 && end <= total, `${scrollTop}: ${start}-${end}`);
  }
  const mid = fonts.fontWindow(10 * fonts.FONT_ROW_PX, 256, total);
  assert.ok(mid.start <= 10 && mid.end >= 10 + 8, 'rows in view are rendered');
  assert.deepEqual(fonts.fontWindow(0, 256, 3), { start: 0, end: 3 });
  assert.deepEqual(fonts.fontWindow(0, 256, 0), { start: 0, end: 0 });
  assert.equal(fonts.scrollToRow(0, 500, 256), 0);
  assert.equal(fonts.scrollToRow(20, 0, 256), 20 * fonts.FONT_ROW_PX + fonts.FONT_ROW_PX - 256);
  assert.equal(fonts.scrollToRow(2, 0, 256), 0, 'already visible → unchanged');
});

test('font picker model stays cheap with a large mock list (rendered rows capped at 50)', () => {
  const big = mk(5000);
  let entries = [];
  const t0 = performance.now();
  for (let i = 0; i < 20; i++) entries = fonts.fontEntries(big, 'family0 mono', false, '');
  const filterMs = (performance.now() - t0) / 20;
  const t1 = performance.now();
  for (let i = 0; i < 1000; i++) fonts.fontWindow(i * 40, 256, entries.length);
  const windowMs = (performance.now() - t1) / 1000;
  console.log(`# font model, 5000 families: filter ${filterMs.toFixed(2)} ms/keystroke, window ${windowMs.toFixed(4)} ms, rendered rows ≤ ${fonts.FONT_MAX_RENDERED}`);
  assert.ok(entries.length > 100);
  assert.ok(filterMs < 60, `filter took ${filterMs} ms`);
  assert.ok(fonts.fontWindow(0, 256, entries.length).end <= fonts.FONT_MAX_RENDERED);
});

test('font picker is wired to the terminal font row and the installed-font command', () => {
  const view = src('ui/views/SettingsView.tsx');
  assert.match(view, /<FontPicker label=\{label\} value=\{settings\.terminalFontFamily \?\? ''\}/);
  assert.match(view, /commands\.fontFamilies\(\)/);
  assert.match(src('ui/views/settings/FontPicker.tsx'), /monoDefault = true/, 'monospace-only is the default for the terminal font');
  assert.match(src('ui/ipc/bindings.ts'), /__TAURI_INVOKE\("font_families"\)/);
});

// ── per-app rules ───────────────────────────────────────────────────────────────────────
test('app rules: add / update / remove keep names unique and untouched rows identical', () => {
  let r = [];
  r = rules.addRule(r, '  Discord ');
  assert.deepEqual(r, [{ app: 'Discord', popup: true, sound: true, urgency: 'app' }]);
  r = rules.addRule(r, 'Firefox');
  r = rules.updateRule(r, 'Discord', { sound: false, urgency: 'low' });
  assert.deepEqual(r[0], { app: 'Discord', popup: true, sound: false, urgency: 'low' });
  assert.deepEqual(r[1], { app: 'Firefox', popup: true, sound: true, urgency: 'app' });
  assert.deepEqual(rules.removeRule(r, 'Discord').map(x => x.app), ['Firefox']);
  assert.equal(rules.appNameError('Discord', r), 'Discord already has a rule');
  assert.equal(rules.appNameError('  ', r), 'Enter an app name');
  assert.equal(rules.appNameError('x'.repeat(257), r), 'At most 256 bytes');
  assert.equal(rules.appNameError('é'.repeat(129), r), 'At most 256 bytes', 'bytes, not characters');
  assert.equal(rules.appNameError('a\nb', r), 'No control characters');
  assert.equal(rules.appNameError('Slack', r), null);
  assert.equal(rules.appNameError('One more', Array.from({ length: 128 }, (_, i) => ({ app: `a${i}`, popup: true, sound: true, urgency: 'app' }))), 'At most 128 rules');
});

test('app rules: suggestions are deduped, newest first, exclude ruled apps, filter and cap', () => {
  const existing = [{ app: 'Slack', popup: true, sound: true, urgency: 'app' }];
  const seen = ['Discord', 'Slack', 'Firefox', 'Discord', ' ', 'Fractal', 'Thunderbird'];
  assert.deepEqual(rules.appSuggestions(seen, existing, ''), ['Discord', 'Firefox', 'Fractal', 'Thunderbird']);
  assert.deepEqual(rules.appSuggestions(seen, existing, 'FR'), ['Fractal']);
  assert.deepEqual(rules.appSuggestions(seen, existing, 'f'), ['Firefox', 'Fractal']);
  assert.deepEqual(rules.appSuggestions(seen, [], '', 2), ['Discord', 'Slack']);
  assert.match(rules.rulesSummary([{ app: 'a', popup: false, sound: false, urgency: 'app' }, { app: 'b', popup: true, sound: false, urgency: 'low' }]), /^2 apps · 1 without popups · 2 muted$/);
  assert.equal(rules.rulesSummary([]), 'Popup, sound and urgency per app');
});

// ── rows / cards ────────────────────────────────────────────────────────────────────────
test('every Nov 6 setting has a searchable row in a card that resets and exports it', () => {
  const nov6 = ['silenceKdePopups', 'notificationSound', 'notificationAppRules', 'approvalsInNotch', 'gameModeDnd', 'gameModeFreezeCompanion', 'gameModePausePollers', 'paletteShortcut', 'openShortcut'];
  const owners = new Map(model.SETTINGS_CARDS.flatMap(c => c.rows.flatMap(r => r.keys.map(k => [k, c]))));
  for (const key of nov6) {
    const card = owners.get(key);
    assert.ok(card, `${key} has a row`);
    assert.notEqual(card.reset, false, `${key} resets with its card`);
    assert.ok(model.resetPatch(card.id, {}) && Object.hasOwn(model.resetPatch(card.id, {}), key), `${key} is in the ${card.id} reset patch`);
  }
  assert.equal(model.rowSpec('silenceKdePopups').label, 'Silence KDE popups');
  assert.match(model.rowSpec('silenceKdePopups').hint, /Monolith shows them; KDE keeps history/);
  assert.deepEqual(model.searchRows('palette', {}).map(g => g.card.id), ['shortcuts']);
  assert.ok(model.searchRows('per-app', {}).some(g => g.rows.some(r => r.id === 'appRules')));
});
