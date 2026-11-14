// Drop shelf: the page-level drop guard (a dropped file must never replace the island page),
// the Shelf page, its adapter, the off switch and the notch wiring (D-Bus, no shell).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const guard = load('ui/island/drop-guard.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

function dragEvent(types) {
  const event = { prevented: false, dataTransfer: { types, dropEffect: 'copy' }, preventDefault() { this.prevented = true; } };
  return event;
}

test('drop guard refuses file and link drags and leaves plain text alone', () => {
  for (const types of [['Files'], ['text/uri-list', 'text/html'], ['text/x-moz-url'], ['_NETSCAPE_URL', 'UTF8_STRING']]) {
    const event = dragEvent(types);
    assert.equal(guard.refuseDrag(event), true, String(types));
    assert.equal(event.prevented, true, String(types));
    assert.equal(event.dataTransfer.dropEffect, 'none', String(types));
  }
  for (const types of [['text/plain'], ['text/html'], [], null]) {
    const event = types === null ? { dataTransfer: null, prevented: false, preventDefault() { this.prevented = true; } } : dragEvent(types);
    assert.equal(guard.refuseDrag(event), false, String(types));
    assert.equal(event.prevented, false, String(types));
  }
});

test('drop guard is installed on the window in the capture phase for enter, over and drop, before the first render', () => {
  const added = [];
  const removed = [];
  const off = guard.installDropGuard({ addEventListener: (t, l, o) => added.push([t, o?.capture]), removeEventListener: (t, l, o) => removed.push([t, o?.capture]) });
  assert.deepEqual(added.map(a => a[0]).sort(), ['dragenter', 'dragover', 'drop']);
  assert.ok(added.every(a => a[1] === true));
  off();
  assert.equal(removed.length, 3);
  const main = src('ui/island-main.tsx');
  assert.match(main, /installDropGuard\(window/);
  assert.ok(main.indexOf('installDropGuard(window') < main.indexOf('beforeFirstRender('), 'guard before the first render');
  assert.doesNotMatch(src('ui/island/drop-guard.ts'), /from '\.\.?\//, 'no relative imports (loaded standalone)');
});

import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
const a = load('ui/views/app-adapters.ts');
const { ShelfApp, shelfDragStart, shelfDragEnd } = load('ui/island/components/ShelfApp.tsx');
const qml = name => src(`../island/plasma-anchor/contents/ui/${name}`);
const row = patch => ({ id: 'file:///data/user/a%20b.txt', name: 'a b.txt', path: '/data/user/a b.txt', uri: 'file:///data/user/a%20b.txt', folder: false, present: true, addedMs: 1, ...patch });
const snap = (...items) => ({ rev: 2, enabled: true, maxItems: 12, items });

test('shelf adapter keeps rows, refuses non-file URIs, control characters and duplicates', () => {
  const model = a.adaptShelf(snap(row(), row({ id: 'file:///d', uri: 'file:///d', name: 'd', path: '/d', folder: true })));
  assert.equal(model.items.length, 2);
  assert.equal(model.items[1].folder, true);
  assert.throws(() => a.adaptShelf(snap(row({ uri: 'https://x/y' }))));
  assert.throws(() => a.adaptShelf(snap(row({ path: '/a\nb' }))));
  assert.throws(() => a.adaptShelf(snap(row(), row())));
  assert.throws(() => a.adaptShelf({ ...snap(), maxItems: 0 }));
});

test('shelf drag-out carries one Rust-encoded URI plus the path, and removes only when asked and taken', () => {
  const data = {}; const dt = { setData: (t, v) => { data[t] = v; }, effectAllowed: '', dropEffect: 'none' };
  shelfDragStart(row(), { dataTransfer: dt });
  assert.deepEqual(data, { 'text/uri-list': 'file:///data/user/a%20b.txt', 'text/plain': '/data/user/a b.txt' });
  assert.equal(dt.effectAllowed, 'copy');
  const ops = [];
  shelfDragEnd(row(), { dataTransfer: { ...dt, dropEffect: 'copy' } }, false, op => ops.push(op));
  shelfDragEnd(row(), { dataTransfer: { ...dt, dropEffect: 'none' } }, true, op => ops.push(op));
  assert.deepEqual(ops, [], 'kept: setting off, or nothing took it');
  shelfDragEnd(row(), { dataTransfer: { ...dt, dropEffect: 'copy' } }, true, op => ops.push(op));
  assert.deepEqual(ops, [{ type: 'remove', id: row().id }]);
});

test('shelf page shows Open, Reveal, Copy path and Remove; a missing file keeps only Copy path and Remove', () => {
  const html = renderToStaticMarkup(createElement(ShelfApp, { model: a.adaptShelf(snap(row(), row({ id: 'file:///gone', uri: 'file:///gone', name: 'gone', path: '/gone', present: false }))), removeAfterDrag: false, intents: { shelf() {}, copy() {} } }));
  for (const label of ['Open a b.txt', 'Reveal a b.txt', 'Copy path of a b.txt', 'Remove a b.txt from the shelf', 'Copy path of gone', 'Remove gone from the shelf']) assert.ok(html.includes(`aria-label="${label}"`), label);
  assert.ok(!html.includes('aria-label="Open gone"') && !html.includes('aria-label="Reveal gone"'));
  assert.match(html, /Moved or deleted/);
  assert.match(html, /draggable="true"/);
  assert.match(html, /Clear all/);
  const empty = renderToStaticMarkup(createElement(ShelfApp, { model: a.adaptShelf(snap()), removeAfterDrag: false, intents: { shelf() {}, copy() {} } }));
  assert.match(empty, /Drag files onto the closed notch/);
  assert.ok(!empty.includes('Clear all'));
});

test('shelf is a compact launcher app with settings rows for every key and the notch wiring uses D-Bus, never the shell', () => {
  assert.match(src('ui/island/IslandSurface.tsx'), /COMPACT_VIEWS[^;]*'shelf'/);
  const model = src('ui/views/settings-model.ts');
  for (const key of ['shelfEnabled', 'shelfMaxItems', 'shelfRemoveAfterDrag']) assert.match(model, new RegExp(`keys: \\['${key}'\\]`));
  const pill = qml('PanelPill.qml'), main = qml('main.qml'), sink = qml('ShelfSink.qml');
  assert.match(pill, /DropArea \{/);
  assert.match(pill, /enabled: pill\.shelfEnabled/);
  assert.match(main, /shelfEnabled: root\.online && root\.shelf/);
  assert.match(main, /onFilesDropped: urls => shelfSink\.add\(urls,/);
  assert.match(sink, /member: "Add"/);
  assert.doesNotMatch(sink, /signature\s*:/, 'Plasma rejects calls with a signature field');
  assert.doesNotMatch(main.slice(main.indexOf('onFilesDropped')), /^.*requests\.enqueue\(.*urls/m, 'paths never go through the shell command');
});
