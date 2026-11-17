import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { adaptMixer } = load('ui/views/app-adapters.ts');
const { VolumeMixer } = load('ui/island/components/VolumeMixer.tsx');
const png = 'data:image/png;base64,iVBORw0KGgo=';
const intents = { mixer() {} };
const stream = { index: 1, sinkIndex: 2, appName: 'Fixture player', binary: null, pid: null, mediaName: null, iconName: 'fixture-player', muted: false, corked: false, volumePercent: 50 };
const raw = patch => ({ rev: 1, available: true, detail: '', defaultSink: 'output', sinks: [{ index: 2, name: 'output', description: 'Output', muted: false, volumePercent: 50, channels: 2, state: 'RUNNING' }], streams: [{ ...stream, ...patch }], apps: [] });
const html = patch => renderToStaticMarkup(createElement(VolumeMixer, { model: adaptMixer(raw(patch)), intents }));
function capture(Component, props) { let tree; function Capture() { tree = Component(props); return tree; } renderToStaticMarkup(createElement(Capture)); return tree; }
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (typeof tree.type === 'function' && !tree.type.prototype?.isReactComponent) return [tree, ...nodes(capture(tree.type, tree.props))];
  return [tree, ...nodes(tree.props?.children)];
}

test('Mixer projects the optional PNG and renders it beside the stream name, not the master', () => {
  assert.equal(adaptMixer(raw({ iconPng: png })).value.streams[0].iconPng, png);
  const rendered = html({ iconPng: png });
  assert.equal((rendered.match(/<img /g) ?? []).length, 1);
  assert.match(rendered, /class="mixer-app-icon" src="data:image\/png;base64,iVBORw0KGgo=" alt="" width="15" height="15"/);
  assert.match(rendered, /<img[^>]+\/><span title="Fixture player">Fixture player<\/span>/);
  assert.match(rendered, /Mute Output/);
  assert.match(rendered, /Mute Fixture player/);
});
test('Mixer keeps its generic music glyph when the optional icon is missing or unresolved', () => {
  const missing = html({});
  assert.equal(missing, html({ iconPng: null }));
  assert.doesNotMatch(missing, /<img/);
  assert.match(missing, /class="fader-name"><svg/);
  assert.equal(adaptMixer(raw({})).value.streams[0].iconPng, null);
});
test('Mixer rejects raw paths, URLs, SVG payloads and malformed or oversized PNG encoding', () => {
  for (const iconPng of ['/icons/player.png', 'file:///icons/player.png', 'https://invalid/player.png', 'fixture-player', 'data:image/svg+xml;base64,AAAA', 'data:image/png;base64,', 'data:image/png;base64,AA=A', 'data:image/png;base64,AAAA\n', 'data:image/png;base64,' + 'A'.repeat(43716), 1, {}]) {
    assert.throws(() => adaptMixer(raw({ iconPng })), /Malformed app snapshot/, String(iconPng).slice(0, 100));
  }
});
test('Mixer decode failure restores the generic glyph using the production error handler', () => {
  const tree = capture(VolumeMixer, { model: adaptMixer(raw({ iconPng: png })), intents });
  const column = nodes(tree).find(n => n.type?.name === 'Column' && !n.props.master);
  assert.ok(column);
  let failed = false;
  function BrokenImage() {
    const rendered = column.type(column.props);
    const hosts = node => !node || typeof node !== 'object' ? [] : Array.isArray(node) ? node.flatMap(hosts) : [node, ...hosts(node.props?.children)];
    const img = hosts(rendered).find(n => n.type === 'img');
    if (img && !failed) { failed = true; img.props.onError(); }
    return rendered;
  }
  const fallback = renderToStaticMarkup(createElement(BrokenImage));
  assert.equal(failed, true);
  assert.doesNotMatch(fallback, /<img/);
  assert.match(fallback, /class="fader-name"><svg/);
});
test('Mixer icon occupies the existing glyph box without changing header or fader sizing', () => {
  const css = readFileSync(new URL('../ui/island/surface.css', import.meta.url), 'utf8');
  assert.match(css, /\.fader-name svg, \.fader-name \.mixer-app-icon \{ width: 15px; height: 15px; flex: none; \}/);
  assert.match(css, /\.fader-name \{ min-height: 32px;/);
  assert.match(css, /\.fader-col \{ min-width: 96px; flex: 1 0 96px;/);
});
