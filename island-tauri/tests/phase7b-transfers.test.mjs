// File-copy progress: the Transfers card on the Downloads page, its adapter, the off switch,
// and the notch wiring (a D-Bus signal in steps, never the appearance push).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const a = load('ui/views/app-adapters.ts');
const { DownloadsApp, TransfersCard, transferAppLabel } = load('ui/island/components/DownloadsApp.tsx');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const ui = name => src(`../island/plasma-anchor/contents/ui/${name}`);
function capture(Component, props) { let tree; function Capture() { tree = Component(props); return tree; } renderToStaticMarkup(createElement(Capture)); return tree; }
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (typeof tree.type === 'function') return [tree, ...nodes(capture(tree.type, tree.props))];
  return [tree, ...nodes(tree.props?.children)];
}
const row = patch => ({ id: '/org/kde/notificationmanager/jobs/JobView_1', app: 'org.kde.dolphin', title: 'Copying', percent: 47,
  processedBytes: 47 * 1024 * 1024, totalBytes: 100 * 1024 * 1024, processedFiles: 1, totalFiles: 1, speed: 3 * 1024 * 1024,
  suspended: false, destination: '/data/user/Videos/movie.mkv', ...patch });
const topic = (...items) => ({ rev: 3, available: true, detail: 'monitoring', items });

test('the adapter keeps real rows, maps an unknown percent to null and hides an off or missing service', () => {
  const [item] = a.adaptTransfers(topic(row({ percent: -1 })));
  assert.equal(item.percent, null);
  assert.equal(item.app, 'org.kde.dolphin');
  assert.deepEqual(a.adaptTransfers(null), []);
  assert.deepEqual(a.adaptTransfers({ ...topic(row()), available: false, detail: 'disabled' }), []);
  for (const patch of [{ percent: 101 }, { percent: -2 }, { title: 'a\nb' }, { id: 'x'.repeat(513) }, { speed: -1 }, { suspended: 'no' }]) {
    assert.throws(() => a.adaptTransfers(topic(row(patch))), /Malformed/);
  }
  assert.throws(() => a.adaptTransfers(topic(row(), row())), /Malformed/, 'duplicate ids');
  assert.throws(() => a.adaptTransfers(topic(...Array.from({ length: 17 }, (_, n) => row({ id: `/j/${n}` })))), /Malformed/, 'at most 16 jobs');
});

test('the card shows title, percent, app, destination, amounts and a stepped bar, with Show in folder and no cancel', () => {
  const items = a.adaptTransfers(topic(row()));
  const html = renderToStaticMarkup(createElement(TransfersCard, { items, onReveal: () => {} }));
  assert.match(html, /Copying · 47%/);
  assert.match(html, /Dolphin → \/data\/user\/Videos\/movie\.mkv/);
  assert.match(html, /47\.0 MB of 100\.0 MB · 3\.0 MB\/s/);
  assert.match(html, /aria-valuenow="47"/);
  assert.match(html, /style="width:47%"/);
  assert.match(html, /Show in folder/);
  assert.doesNotMatch(html, /Cancel|Stop/i, 'a third party cannot cancel a KDE job');
  const calls = [];
  const tree = capture(TransfersCard, { items, onReveal: id => calls.push(id) });
  nodes(tree).find(n => n.props?.['aria-label'] === 'Show Copying in folder').props.onClick();
  assert.deepEqual(calls, ['/org/kde/notificationmanager/jobs/JobView_1']);
});

test('no destination: no folder button; paused and file-count jobs read plainly; no jobs: no card', () => {
  const items = a.adaptTransfers(topic(row({ destination: '', totalBytes: 0, processedFiles: 2, totalFiles: 5, suspended: true, percent: -1 })));
  const html = renderToStaticMarkup(createElement(TransfersCard, { items, onReveal: () => {} }));
  assert.doesNotMatch(html, /Show in folder/);
  assert.match(html, /2 of 5 files · Paused/);
  assert.doesNotMatch(html, /%</, 'no percent while the job has not reported one');
  assert.equal(renderToStaticMarkup(createElement(TransfersCard, { items: [], onReveal: () => {} })), '');
  assert.equal(transferAppLabel('org.kde.dolphin'), 'Dolphin');
  assert.equal(transferAppLabel('kio'), 'Kio');
  assert.equal(transferAppLabel(''), 'KDE');
});

test('the Downloads page puts the card above the list and wires the reveal intent', () => {
  const download = { name: 'a.pdf', mtimeMs: 1, sizeBytes: 1, kind: 'document', state: 'ready', ageLabel: 'Now' };
  const html = renderToStaticMarkup(createElement(DownloadsApp, { items: [download], transfers: a.adaptTransfers(topic(row())), intents: { download() {}, transfer() {} } }));
  assert.ok(html.indexOf('Transfers') < html.indexOf('a.pdf'));
  const plain = renderToStaticMarkup(createElement(DownloadsApp, { items: [download], intents: { download() {} } }));
  assert.doesNotMatch(plain, /Transfers/, 'without jobs the page is unchanged');
  const view = src('ui/views/AppsView.tsx');
  assert.match(view, /transfer: reveal/);
  assert.match(view, /appApi\.transferReveal\(id\)/);
});

test('the off switch has a Settings row next to the other notch states', () => {
  const model = src('ui/views/settings-model.ts');
  assert.match(model, /\{ id: 'copyProgress', label: 'File-copy progress', hint: '[^']+', keys: \['notchCopyProgress'\] \}/);
  assert.match(model, /rows: rows\('everyMonitor', 'notchMonitors', 'waitingPulse', 'hoverPeek', 'copyProgress',/, 'on the Notch card, after hover peek');
});

test('the notch gets progress as a stepped D-Bus signal, not through the appearance push', () => {
  const main = ui('main.qml'), source = ui('CopyProgressSource.qml'), pill = ui('PanelPill.qml');
  assert.match(main, /CopyProgressSource \{ id: copySource; active: root\.copyProgress \}/);
  assert.match(main, /copyPercent: root\.online && root\.copyProgress \? copySource\.percent : -1/);
  assert.doesNotMatch(main, /live\.copy|appearance\.copyPercent/, 'the percent never rides the appearance payload');
  assert.match(source, /iface: "local\.hermes\.Monolith\.Transfers"/);
  assert.match(source, /function dbusCopyProgress\(/);
  assert.match(source, /member: "Progress"/);
  assert.doesNotMatch(source, /signature:/, 'Plasma rejects calls that carry an explicit signature');
  const copyBlock = pill.slice(pill.indexOf('property int copyPercent'), pill.indexOf('readonly property bool motion'));
  assert.match(copyBlock, /copyText/);
  assert.doesNotMatch(source + copyBlock + pill, /Timer \{[^}]*copy|NumberAnimation[^}]*copy|Behavior on copy/, 'static text: nothing animates');
  assert.match(pill, /copyLive: animateHere && !companionFrozen/, 'holds still where the pointer is not');
  const companion = src('src-tauri/src/integrations/plasma/companion.rs');
  assert.match(companion, /fn copy_progress\(emitter: &SignalEmitter<'_>, percent: i32\)/);
  const runtime = src('src-tauri/src/integrations/plasma/runtime.rs');
  assert.match(runtime, /"copyProgress":settings\.notch_copy_progress/);
});
