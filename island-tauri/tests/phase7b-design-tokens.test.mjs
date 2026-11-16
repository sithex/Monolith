// Design-token lint: the island CSS may only use the type / weight / radius scale and the text
// roles defined in ui/island/tokens/themes.css. Measured exceptions are listed here by selector.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(fileURLToPath(import.meta.url), '..', '..');
const read = (path) => readFileSync(join(root, path), 'utf8');
function cssFiles(dir = join(root, 'ui'), out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name), rel = relative(root, full).split('\\').join('/');
    if (statSync(full).isDirectory()) { if (rel !== 'ui/legacy') cssFiles(full, out); }
    else if (name.endsWith('.css') && rel !== 'ui/styles/spike.css') out.push(rel);
  }
  return out.sort();
}

// Selectors whose values are measured against a reference and stay off the scale:
// timer digits and ruler, the notch clock replica, the layout-editor miniature, hero numerals on
// Updates, the terminal, launcher tiles, mixer faders, the island shell and side buttons, lyrics
// lines, the companion preview and the Now playing backdrop.
const EXCEPTION = /\.countdown|\.wheel-|notch-clock|\.le-mini|\.le-notch|\.updates-total|\.updates-check|\.xterm|\.app-tile|\.tile-badge|\.fader-track|\.fader-fill|\.island-shell|\.live-side-button|\.am-lyrics|\.am-plain|\.companion-|\.np-backdrop|@font-face/;

function declarations() {
  const out = [];
  for (const file of cssFiles()) {
    const css = read(file).replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, ' '));
    for (const block of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selector = block[1].trim().replace(/\s+/g, ' ');
      const line = css.slice(0, block.index + block[1].length).split('\n').length;
      for (const decl of block[2].split(';')) {
        const at = decl.indexOf(':');
        if (at > 0) out.push({ file, line, selector, prop: decl.slice(0, at).trim(), value: decl.slice(at + 1).trim(), exempt: EXCEPTION.test(selector) });
      }
    }
  }
  return out;
}
const all = declarations();
const where = (d) => `${d.file}:${d.line} [${d.selector.slice(0, 70)}] ${d.prop}: ${d.value}`;
const tokens = Object.fromEntries([...read('ui/island/tokens/themes.css').matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));

test('the scale itself is the short list', () => {
  const named = (prefix) => Object.keys(tokens).filter((name) => name.startsWith(prefix)).map((name) => `${name.slice(prefix.length)}=${tokens[name]}`).sort();
  assert.deepEqual(named('--fs-'), ['2xl=28px', 'base=14px', 'body=13px', 'lg=18px', 'sm=11px', 'title=15px', 'xl=22px']);
  assert.deepEqual(named('--radius-'), ['3xl=16px', '4xl=20px', 'full=999px', 'lg=10px', 'sm=6px']);
  assert.deepEqual(named('--space-'), ['1=4px', '2=8px', '3=12px', '4=16px', '5=24px']);
  assert.deepEqual(named('--fw-'), ['regular=400', 'strong=600']);
});

test('font sizes come from the type scale', () => {
  const bad = [];
  for (const d of all) {
    if (d.exempt || (d.prop !== 'font-size' && d.prop !== 'font')) continue;
    const size = d.prop === 'font' ? d.value.split('/')[0] : d.value;
    if (/clamp\(|calc\(/.test(size)) { bad.push(where(d)); continue; }
    // A token with a px fallback (windows that load before the token file) is still a token.
    if (/(^|[\s(])\d*\.?\d+px/.test(size.replace(/var\(--fs-[\w-]+, \d+px\)/g, 'TOKEN'))) bad.push(where(d));
    for (const used of size.matchAll(/var\((--fs-[\w-]+)/g)) if (!(used[1] in tokens)) bad.push(where(d));
  }
  assert.deepEqual(bad, []);
});

test('font weights are 400 or 600', () => {
  const bad = [];
  for (const d of all) {
    if (d.exempt) continue;
    if (d.prop === 'font-weight' && !/^(400|600|inherit|normal|var\(--fw-(regular|strong)\))( !important)?$/.test(d.value)) bad.push(where(d));
    if (d.prop === 'font' && /(^|\s)(100|200|300|500|550|650|700|750|800|900|bold)\s/.test(d.value.split('/')[0])) bad.push(where(d));
  }
  assert.deepEqual(bad, []);
});

test('corner radii come from the radius scale', () => {
  // Allowed raw values: 0, hairline bars up to 3px (also as compound corners), 50 % circles,
  // and compound shapes that hang from an edge.
  const hairline = /^(0|[123]px|50%|inherit)( (0|[123]px))*( !important)?$/;
  const bad = [];
  for (const d of all) {
    if (d.exempt || d.prop !== 'border-radius' || hairline.test(d.value)) continue;
    if (/^0 0 \d+px \d+px$/.test(d.value)) continue;
    const stripped = d.value.replace(/var\((--radius-[\w-]+)(, \d+px)?\)/g, (whole, name) => (name in tokens ? 'TOKEN' : whole));
    if (/\dpx|var\(--radius-/.test(stripped.replace(/TOKEN - \d+px/, 'TOKEN'))) bad.push(where(d));
  }
  assert.deepEqual(bad, []);
});

test('text never uses the scheme comment colour or a hard-coded danger red', () => {
  const bad = all.filter((d) => d.prop === 'color' && /--(ui|skin)-dim\b|--v-dim\b|--(ui|skin)-error\b|--v-red\b/.test(d.value)).map(where);
  assert.deepEqual(bad, []);
  assert.equal(all.some((d) => /var\(--v-dim\)/.test(d.value)), false);
  assert.equal(tokens['--ui-danger'], 'var(--ui-error)');
});

// WCAG contrast of hint text, composited from the token's own opacity, for every bundled scheme.
const channel = (c) => { const v = c / 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
const luminance = ([r, g, b]) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
const contrast = (a, b) => { const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x); return (hi + 0.05) / (lo + 0.05); };
const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const over = (fg, bg, alpha) => bg.map((c, i) => c + (fg[i] - c) * alpha);
export function hintContrastTable() {
  const source = read('ui/island/tokens/schemes.ts');
  const constants = Object.fromEntries([...source.matchAll(/const (\w+): SchemeRoles = roles\(([^)]*)\)/g)].map((m) => [m[1], m[2]]));
  const alpha = (name) => Number(/var\(--ui-ink\) (\d+)%/.exec(tokens[name])[1]) / 100;
  const light = /html\[data-tone='light'\]\s*\{[^}]*--ui-ink:\s*(#[0-9a-f]{6})/i.exec(read('ui/island/tokens/light-tone.css'))[1];
  const rows = [];
  // One chunk per scheme entry; skin, custom and album have no fixed palette (palette: null).
  for (const chunk of source.split(/\{ id: '/).slice(1)) {
    const m = /^([\w-]+)'[\s\S]*?palette: (?:roles\(([^)]*)\)|(\w+))/.exec(chunk);
    if (!m || m[3] === 'null') continue;
    const hexes = (m[2] ?? constants[m[3]]).match(/#[0-9a-f]{6}/gi);
    const page = rgb(hexes[0]), text = rgb(hexes[4]);
    const isLight = (0.2126 * page[0] + 0.7152 * page[1] + 0.0722 * page[2]) / 255 > 0.55;
    const ink = rgb(isLight ? light : tokens['--ui-ink']), overlay = isLight ? text : [255, 255, 255];
    const fills = { page, card: over(overlay, page, 0.065), selected: over(overlay, page, 0.12) };
    const row = { id: m[1], tone: isLight ? 'light' : 'dark' };
    for (const [name, fill] of Object.entries(fills)) row[name] = contrast(over(ink, fill, alpha('--ui-text-hint')), fill);
    rows.push(row);
  }
  return rows;
}

test('hint text reaches 4.5:1 on page, card and selected-card fills in every bundled scheme', () => {
  const rows = hintContrastTable();
  assert.equal(rows.length, 26);
  const failing = rows.flatMap((row) => ['page', 'card', 'selected'].filter((fill) => row[fill] < 4.5).map((fill) => `${row.id} ${fill} ${row[fill].toFixed(2)}`));
  assert.deepEqual(failing, []);
});
