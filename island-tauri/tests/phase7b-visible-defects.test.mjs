import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
import { driver, component } from './phase7b-runtime-harness.mjs';

const { formatTemperature, relativeTime } = load('ui/island/formatters.ts');
const adapters = load('ui/views/app-adapters.ts');
const { QuickInfo } = load('ui/island/components/QuickInfo.tsx');
const { ControlsApp } = load('ui/island/components/ControlsApp.tsx');
const { checkedLabel } = load('ui/island/components/UpdatesApp.tsx');
const { rowSpec } = load('ui/views/settings-model.ts');
const now = Date.parse('2026-11-15T18:00:00Z');
const render = (C, props) => renderToStaticMarkup(createElement(C, props));
const notification = (receivedAt = now - 120000) => ({ rev: 1, id: 'fixture', source: 'system', appName: 'Fixture', title: 'Notice', body: null, iconPng: null, urgency: 'normal', receivedAt, actions: [], canInvokeActions: false, progress: null });
const download = (mtimeMs = now - 3600000) => ({ rev: 1, available: true, detail: 'ready', items: [{ name: 'fixture.pdf', mtimeMs, sizeBytes: 1, kind: 'document', state: 'ready' }] });
const weather = (temp, unit) => ({ rev: 1, available: true, stale: false, tempC: 16.1, temp, unit, code: 0, icon: null, isDay: true, updatedAt: now, place: null });

// Execute the actual AppsView adapter calls, not a duplicated presentation pipeline.
function appHarness(notes, files) {
  const { createEpochTopic } = load('ui/stores/epoch-topic.ts');
  const appStores = { calendar: createEpochTopic(), timer: createEpochTopic() };
  const h = driver({
    'ui/ipc/client.ts': { mediaSetVisible: async () => {} },
    'ui/stores/index.ts': { useSettings: () => ({ dnd: false, redactBodies: false }) },
    'ui/ipc/notifications.ts': { useNotifications: () => notes },
    'ui/stores/apps.ts': {
      appStores, clipboardHistory: { setVisible() {} }, useClipboardHistory: () => ({}),
      useApp: key => key === 'downloads' ? files : null,
      useMedia: () => null, useLyrics: () => null, useEqualizer: () => null,
      useWeather: () => null, usePower: () => null, useBluetooth: () => null, serviceStatus: () => null,
    },
  });
  const { AppsView } = h.load('ui/views/AppsView.tsx');
  return { ...h, view: view => h.render(AppsView, { active: true, view }) };
}

test('visible defects: temperature rounds whole Fahrenheit and Celsius degrees, including negatives', () => {
  for (const [temp, unit, expected] of [[60.980000000000004, 'fahrenheit', '61°F'], [16.1, 'celsius', '16°C'], [-8.6, 'celsius', '-9°C'], [-0.2, 'celsius', '0°C'], [0, 'fahrenheit', '0°F']]) {
    assert.equal(formatTemperature(temp, unit), expected);
    const model = adapters.adaptQuickInfo(weather(temp, unit), null, [], { weather: '', power: 'Unavailable', bluetooth: null });
    assert.match(render(QuickInfo, { model }), new RegExp(`<strong>${expected}</strong>`));
  }
});

test('visible defects: missing and invalid temperatures never leak null, NaN or Infinity', () => {
  for (const temp of [null, undefined, NaN, Infinity, -Infinity]) assert.equal(formatTemperature(temp, 'celsius'), 'Unknown temperature');
  const model = adapters.adaptQuickInfo(weather(null, 'celsius'), null, [], { weather: '', power: 'Unavailable', bluetooth: null });
  assert.match(render(QuickInfo, { model }), /Unknown temperature/);
  for (const temp of [NaN, Infinity]) assert.throws(() => adapters.adaptQuickInfo(weather(temp, 'celsius'), null, [], { weather: '', power: '', bluetooth: null }), /Malformed/);
});

test('visible defects: relative time uses epoch milliseconds and ISO instants with boundary-safe ages', () => {
  for (const [age, expected] of [[0, 'just now'], [59999, 'just now'], [60000, '1 min ago'], [120000, '2 min ago'], [3599999, '59 min ago'], [3600000, '1 hr ago'], [86399999, '23 hr ago'], [86400000, '1 days ago'], [259200000, '3 days ago']]) {
    assert.equal(relativeTime(now - age, now), expected);
    assert.equal(relativeTime(new Date(now - age).toISOString(), now), expected);
  }
  assert.equal(relativeTime('2026-11-15T13:00:00-04:00', now), '1 hr ago');
  assert.equal(relativeTime('2026-08-09T06:39:51.1Z', now), '98 days ago');
  assert.equal(relativeTime(0, 60000), '1 min ago', 'epoch zero is valid, not a missing timestamp');
});

test('visible defects: missing/invalid timestamps have a placeholder; clock skew clamps to just now', () => {
  for (const value of [null, undefined, '', 'invalid', '1', '2026-99-99T00:00:00Z', '0001-01-01T00:00:00Z', NaN, Infinity, -Infinity, -1]) {
    assert.equal(relativeTime(value, now), 'Age unavailable');
    assert.equal(relativeTime(value, now, 'Unknown'), 'Unknown');
  }
  assert.equal(relativeTime(now + 60000, now), 'just now');
  assert.equal(relativeTime(now, NaN), 'Age unavailable');
});

test('visible defects: extracted Updates age helper preserves its existing wording', () => {
  for (const [seconds, expected] of [[null, 'No successful check yet'], [NaN, 'No successful check yet'], [-1, 'No successful check yet'], [0, 'Checked just now'], [60, 'Checked 1 min ago'], [3600, 'Checked 1 hr ago'], [86400, 'Checked 1 days ago'], [8640000, 'Checked 100 days ago']]) assert.equal(checkedLabel(seconds), expected);
});

test('visible defects: both adapters use the supplied relative-time formatter and reject invalid wire timestamps', () => {
  assert.equal(adapters.adaptDownloads(download(), at => relativeTime(at, now)).value[0].ageLabel, '1 hr ago');
  assert.equal(adapters.adaptNotifications([notification()], false, false, at => relativeTime(at, now)).items[0].ageLabel, '2 min ago');
  for (const value of [null, undefined, NaN, Infinity, -1, 'invalid']) {
    assert.throws(() => adapters.adaptDownloads({ ...download(), items: [{ ...download().items[0], mtimeMs: value }] }, relativeTime), /Malformed/);
    assert.throws(() => adapters.adaptNotifications([{ ...notification(), receivedAt: value }], false, false, relativeTime), /Malformed/);
  }
});

test('visible defects: production AppsView passes relative time to Notifications', t => {
  t.mock.method(Date, 'now', () => now);
  const h = appHarness([notification()], null);
  try { assert.equal(component(h.view('notifications'), 'NotificationsView').props.model.items[0].ageLabel, '2 min ago'); }
  finally { h.dispose(); }
});

test('visible defects: production AppsView passes relative time to Downloads', t => {
  t.mock.method(Date, 'now', () => now);
  const h = appHarness([], download());
  try { assert.equal(component(h.view('downloads'), 'DownloadsApp').props.items[0].ageLabel, '1 hr ago'); }
  finally { h.dispose(); }
});

test('visible defects: Controls renders relative peer ages and unknown for missing/invalid dates', t => {
  t.mock.method(Date, 'now', () => now);
  for (const online of [false, null, true]) {
    for (const lastSeen of ['2026-11-15T17:00:00Z', null, '', 'not-a-date', '0001-01-01T00:00:00Z']) {
      const model = { toggles: null, tiles: { output: { available: false, detail: 'Unavailable' }, network: { available: false, detail: 'Unavailable' }, tailscale: { available: true, peers: [{ hostName: 'Fixture', os: '', online, lastSeen }] }, voice: { available: false, detail: 'Unavailable' } } };
      const html = render(ControlsApp, { model, intents: {} });
      if (online) assert.doesNotMatch(html, /Last seen:/);
      else {
        assert.match(html, new RegExp(`${online === false ? 'Offline' : 'Online status unknown'} · Last seen: ${lastSeen?.startsWith('2026') ? '1 hr ago' : 'Unknown'}`));
        assert.doesNotMatch(html, /2026-11|not-a-date|0001-01/);
      }
    }
  }
});

test('visible defects: Settings consistently uses colour without changing setting ids or keys', () => {
  assert.equal(rowSpec('notchColor').label, 'Notch colour');
  assert.deepEqual(rowSpec('notchColor').keys, ['panelClockColor']);
  assert.equal(rowSpec('notchBorderColor').label, 'Notch border colour');
  assert.deepEqual(rowSpec('notchBorderColor').keys, ['notchBorderColor']);
});
