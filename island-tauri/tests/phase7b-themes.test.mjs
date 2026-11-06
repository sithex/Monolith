// Nov 6: community colour schemes (Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night, Rosé Pine,
// One Dark, Solarized), the family picker model, terminal ANSI palettes and the light tone.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { SCHEMES, schemeFamilies, initialOpenFamily, navTarget, isLightPalette, schemePalette } = load('ui/island/tokens/schemes.ts');
const { xtermTheme } = load('ui/views/terminal-colours.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const HEX = /^#[0-9a-f]{6}$/;
const ROLES = ['background', 'surface', 'border', 'dim', 'text', 'primary', 'accent', 'error'];
const COMMUNITY_FAMILIES = ['catppuccin', 'dracula', 'nord', 'gruvbox', 'tokyonight', 'rosepine', 'onedark', 'solarized'];
const community = SCHEMES.filter(s => COMMUNITY_FAMILIES.includes(s.family));
const LIGHT_IDS = ['catppuccin-latte', 'gruvbox-light', 'rose-pine-dawn', 'solarized-light'];
const ORIGINAL = ['skin', 'green', 'neutral', 'ocean', 'dune', 'dusk', 'silk', 'aurora', 'ember', 'rose', 'glacier', 'custom', 'album'];

// WCAG 2.x relative-luminance contrast.
const lum = hex => { const [r, g, b] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255).map(v => v <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4); return .2126 * r + .7152 * g + .0722 * b; };
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + .05) / (y + .05); };

test('every scheme id is unique and matches the Rust Appearance union in both directions', () => {
  const ids = SCHEMES.map(s => s.id);
  assert.equal(new Set(ids).size, ids.length);
  const union = src('ui/ipc/bindings.ts').replace(/\/\*\*[\s\S]*?\*\//g, '').match(/export type Appearance = ([^;]+);/)[1];
  const members = [...union.matchAll(/"([^"]+)"/g)].map(m => m[1]);
  assert.deepEqual([...members].sort(), [...ids].sort(), 'schemes.ts and the Rust enum list the same ids');
  for (const id of ORIGINAL) assert.ok(ids.includes(id), `${id} is still offered`);
  assert.equal(community.length, 16, 'Catppuccin ×4, Dracula, Nord, Gruvbox ×2, Tokyo Night ×2, Rosé Pine ×3, One Dark, Solarized ×2');
});

test('community palettes: valid hex, every role present, 16-colour ANSI', () => {
  for (const s of community) {
    assert.ok(s.palette, s.id);
    for (const role of ROLES) assert.match(s.palette[role], HEX, `${s.id}.${role}`);
    assert.equal(s.ansi16?.length, 16, `${s.id} ansi16`);
    for (const c of s.ansi16) assert.match(c, HEX, `${s.id} ansi16 ${c}`);
    s.dots.forEach(c => assert.match(c, HEX, `${s.id} dots`));
    assert.ok(s.name && s.note, s.id);
    assert.equal(s.note.includes('light'), LIGHT_IDS.includes(s.id), `${s.id} note flags light schemes`);
  }
});

test('contrast: text ≥ 4.5 and dim ≥ 3 on background and surface; accent and error ≥ 3 on background', () => {
  const rows = [];
  for (const s of community) {
    const p = s.palette;
    const r = { id: s.id, text: Math.min(contrast(p.text, p.background), contrast(p.text, p.surface)), dim: Math.min(contrast(p.dim, p.background), contrast(p.dim, p.surface)),
      accent: contrast(p.accent, p.background), error: contrast(p.error, p.background) };
    rows.push(r);
    assert.ok(r.text >= 4.5, `${s.id} text ${r.text.toFixed(2)}`);
    assert.ok(r.dim >= 3, `${s.id} dim ${r.dim.toFixed(2)}`);
    assert.ok(r.accent >= 3, `${s.id} accent ${r.accent.toFixed(2)}`);
    assert.ok(r.error >= 3, `${s.id} error ${r.error.toFixed(2)}`);
  }
  if (process.env.SHOW_CONTRAST) console.log(rows.map(r => `${r.id.padEnd(22)} text ${r.text.toFixed(2)} dim ${r.dim.toFixed(2)} accent ${r.accent.toFixed(2)} error ${r.error.toFixed(2)}`).join('\n'));
});

test('Hermes skin sync: every community scheme is a valid skin payload', () => {
  const { themePayload } = load('ui/island/hermes-theme.ts');
  for (const s of community) {
    const payload = themePayload(s.palette);
    assert.ok(payload, s.id);
    for (const role of ROLES) assert.equal(payload[role], s.palette[role].toLowerCase(), `${s.id}.${role}`);
  }
});

test('light tone: exactly the four light schemes (and no original one) are light', () => {
  const light = SCHEMES.filter(s => s.palette && isLightPalette(s.palette)).map(s => s.id);
  assert.deepEqual(light.sort(), [...LIGHT_IDS].sort());
  assert.equal(isLightPalette({ background: 'var(--skin-background)' }), false, 'non-hex backgrounds are never light');
});

test('light-tone.css has an override for every white-overlay rule in the island CSS', () => {
  const light = src('ui/island/tokens/light-tone.css');
  const strip = css => css.replace(/\/\*[\s\S]*?\*\//g, '');
  const missing = [];
  for (const f of readdirSync(new URL('../ui/island/', import.meta.url)).filter(f => f.endsWith('.css'))) {
    for (const m of strip(src(`ui/island/${f}`)).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      if (!/rgb\(255 255 255/.test(m[2]) || m[1].trim().startsWith('@')) continue;
      for (const sel of m[1].split(',').map(s => s.trim())) {
        // `:root` rules are covered by the variable block at the top of light-tone.css.
        const want = `html[data-tone='light']${sel.startsWith('html[data-glass') ? sel.replace('html', '') : ` ${sel}`}`;
        // Whole-selector match: `.fader-fill` must not be satisfied by `.fader-fill-x` or `.fader-fill[data-muted]`.
        if (sel !== ':root' && !new RegExp(`(^|\\n|,\\s*)${want.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*(,|\\{)`).test(light)) missing.push(`${f}: ${sel}`);
      }
    }
  }
  assert.deepEqual(missing, []);
  for (const v of ['--v-fill', '--v-fill-hi', '--v-fill-soft', '--v-stroke', '--v-stroke-hi', '--v-hover', '--v-track', '--v-dim']) assert.match(light, new RegExp(`${v}:`));
});

test('inline white fallbacks (tile icons, fader glyph) are CSS variables the light tone redefines', () => {
  const light = src('ui/island/tokens/light-tone.css');
  assert.match(src('ui/island/Launcher.tsx'), /var\(--tile-ink, rgb\(255 255 255 \/ \.85\)\)/);
  assert.match(src('ui/island/components/VolumeMixer.tsx'), /var\(--fader-ink, rgb\(255 255 255 \/ \.6\)\)/);
  assert.match(light, /--tile-ink:/); assert.match(light, /--fader-ink:/);
});

test('the original schemes keep xterm exactly as before (no ANSI override, same four keys)', () => {
  // The pre-Nov 6 theme(): transparent background and three palette colours, xterm's own ANSI 16.
  const legacy = p => ({ background: '#00000000', foreground: p.text, cursor: p.accent, selectionBackground: p.border });
  for (const id of ORIGINAL) {
    assert.equal(SCHEMES.find(s => s.id === id).ansi16, undefined, `${id} has no ansi16`);
    const p = schemePalette(id, {}) ?? { background: '#101010', text: '#dddddd', accent: '#eeeeee', border: '#444444' };
    assert.deepEqual(xtermTheme(p), legacy(p), id);
    assert.deepEqual(xtermTheme(p, undefined), legacy(p), id);
  }
});

test('community schemes hand xterm their official ANSI 16 and keep the background see-through', () => {
  for (const s of community) {
    const t = xtermTheme(s.palette, s.ansi16);
    assert.equal(t.black, s.ansi16[0]); assert.equal(t.red, s.ansi16[1]); assert.equal(t.white, s.ansi16[7]);
    assert.equal(t.brightBlack, s.ansi16[8]); assert.equal(t.brightWhite, s.ansi16[15]);
    assert.equal(t.background, `${s.palette.background}00`, `${s.id}: alpha 00 keeps the island surface visible`);
    assert.equal(t.foreground, s.palette.text);
    assert.equal(t.cursor, s.palette.accent);
  }
  assert.equal(xtermTheme(community[0].palette, community[0].ansi16.slice(0, 15)).black, undefined, 'a malformed list is ignored');
});

test('picker model: one row per family, Monolith presets grouped, variants in order', () => {
  const families = schemeFamilies();
  assert.equal(new Set(families.map(f => f.id)).size, families.length);
  assert.deepEqual(families.flatMap(f => f.schemes.map(s => s.id)), SCHEMES.map(s => s.id), 'every scheme appears once, in SCHEMES order');
  const by = id => families.find(f => f.id === id);
  assert.deepEqual(by('catppuccin').schemes.map(s => s.name), ['Mocha', 'Macchiato', 'Frappé', 'Latte']);
  assert.deepEqual(by('tokyonight').schemes.map(s => s.name), ['Night', 'Storm']);
  assert.deepEqual(by('rosepine').schemes.map(s => s.name), ['Main', 'Moon', 'Dawn']);
  assert.deepEqual(by('gruvbox').schemes.map(s => s.name), ['Dark', 'Light']);
  assert.deepEqual(by('solarized').schemes.map(s => s.name), ['Dark', 'Light']);
  assert.equal(by('catppuccin').name, 'Catppuccin'); assert.equal(by('rosepine').name, 'Rosé Pine');
  assert.deepEqual(by('monolith').schemes.map(s => s.id), ['green', 'neutral', 'ocean', 'dune', 'dusk', 'silk', 'aurora', 'ember', 'rose', 'glacier']);
  for (const id of ['dracula', 'nord', 'onedark']) { assert.equal(by(id).schemes.length, 1, `${id} is a plain row`); assert.equal(by(id).group, 'community'); }
  for (const id of ['skin', 'custom', 'album']) { assert.equal(by(id).schemes.length, 1); assert.equal(by(id).group, 'monolith'); }
  assert.equal(families[0].id, 'skin', 'Hermes skin stays first');
  assert.ok(families.findIndex(f => f.id === 'monolith') < families.findIndex(f => f.id === 'catppuccin'), 'Monolith group precedes the community ones');
  assert.equal(families.filter(f => f.group === 'monolith').at(-1).id, 'album', 'community rows come after every Monolith row');
});

test('picker model: the active scheme\'s family starts expanded', () => {
  const families = schemeFamilies();
  assert.equal(initialOpenFamily('catppuccin-latte', families), 'catppuccin');
  assert.equal(initialOpenFamily('rose-pine-dawn', families), 'rosepine');
  assert.equal(initialOpenFamily('ocean', families), 'monolith');
  assert.equal(initialOpenFamily('green', families), 'monolith');
  assert.equal(initialOpenFamily('dracula', families), null, 'a single-variant family has nothing to expand');
  assert.equal(initialOpenFamily('skin', families), null);
  assert.equal(initialOpenFamily('album', families), null);
});

test('picker keyboard model: arrows, Home and End move through the visible rows', () => {
  assert.equal(navTarget('ArrowDown', 2, 5), 3); assert.equal(navTarget('ArrowDown', 4, 5), 4);
  assert.equal(navTarget('ArrowUp', 2, 5), 1); assert.equal(navTarget('ArrowUp', 0, 5), 0);
  assert.equal(navTarget('Home', 3, 5), 0); assert.equal(navTarget('End', 1, 5), 4);
  assert.equal(navTarget('a', 1, 5), null); assert.equal(navTarget('ArrowDown', 0, 0), null);
});

test('picker markup: disclosure semantics, one open family, reduced motion', () => {
  const picker = src('ui/island/SchemePicker.tsx');
  assert.match(picker, /aria-expanded=\{isOpen\}/);
  assert.match(picker, /aria-controls=/);
  assert.match(picker, /setOpen\(isOpen \? null : f\.id\)/, 'a single `open` id: opening one family closes the other');
  assert.match(picker, /ArrowRight/); assert.match(picker, /ArrowLeft/);
  assert.match(picker, /reduceMotion === true/); assert.match(picker, /duration: 0/);
  assert.match(picker, /<button type="button"/, 'real buttons: Enter and Space toggle natively');
  assert.match(src('ui/island/tokens/scheme-picker.css'), /data-reduce-motion='true'\] \.scheme-chevron/);
});
