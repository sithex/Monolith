// Service "unavailable" states: every code the Rust services publish has a plain sentence,
// unknown codes stay diagnosable, healthy backend names are never shown as errors.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const { describeServiceReason, SERVICE_REASONS, HEALTHY_DETAILS, GENERIC_REASON } = load('ui/island/service-reasons.ts');
const { ServiceNotice, ServiceCard } = load('ui/island/components/AppPrimitives.tsx');
const { TimerApp } = load('ui/island/components/TimerApp.tsx');
const html = (type, props) => renderToStaticMarkup(createElement(type, props));
const CODE = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;

const LOADING = ['starting', 'fetching'];
const OFF = ['disabled', 'idle'];
const SETUP = ['not-configured', 'no-location'];
const ERRORS = ['stopped', 'fetch-failed', 'no-owner', 'name-lost', 'bus-unavailable', 'no-system-bus-configured',
  'no-session-bus-configured', 'no-adapter', 'unsupported-platform', 'unsupported-system', 'unsupported',
  'directory-unavailable', 'watcher-unavailable', 'watch-failed', 'watch-error', 'scan-failed', 'capture-ended',
  'unreadable', 'save-failed', 'no-notification-owner', 'monitor-permission-denied', 'monitor-state-poisoned',
  'tailscale-unavailable', 'voice-runner-unavailable', 'no-fixture-database', 'load-failed-file-preserved',
  'local-load-failed-file-preserved', 'no-private-system-bus', 'pactl-unavailable', 'checks-unavailable'];
const HEALTHY = ['inotify', 'lrclib', 'pactl-subscribe', 'dbus', 'ready', 'fresh', 'monitoring', 'ext-data-control',
  'wlr-data-control', 'power-profiles-daemon', 'controls', 'saved-voices', 'read-only-status', 'owner-present',
  'read-only-checks', 'default-source', 'system-tiles'];

/** Code-shaped literals in production Rust: unavailable("…"), fail("…"), empty("…"), and any `…detail`
 *  field set to a literal directly (`detail: "…"`, `mic_detail = "…"`) or through `if … { "a" } else { "b" }`. */
function rustCodes() {
  const found = new Map();
  const walk = dir => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) { if (name !== 'tests') walk(path); continue; }
      if (!name.endsWith('.rs') || /(^|_)tests?\.rs$|^tests?_/.test(name)) continue;
      // Inline test modules sit at the end of a file; their literals are fixtures, not published codes.
      const source = readFileSync(path, 'utf8').split(/#\[cfg\(test\)\]\s*\n\s*mod\s/)[0];
      for (const m of source.matchAll(/\b(?:unavailable|fail|empty)\("([^"]*)"|\b\w*detail(?::|\s*=)\s*"([^"]*)"|\b\w*detail(?::|\s*=)\s*if\b[^{;]*\{\s*"([^"]*)"\s*\}\s*else\s*\{\s*"([^"]*)"/g)) {
        for (const code of m.slice(1)) if (code !== undefined && CODE.test(code)) found.set(code, path.slice(root.length));
      }
    }
  };
  walk(join(root, 'src-tauri/src'));
  return found;
}

test('service reasons: every listed loading/off/setup/error code has an explicit entry of the right kind', () => {
  for (const [codes, kind] of [[LOADING, 'loading'], [OFF, 'off'], [SETUP, 'setup'], [ERRORS, 'error']]) {
    for (const code of codes) {
      assert.ok(Object.hasOwn(SERVICE_REASONS, code), `${code} has no entry`);
      const r = describeServiceReason(code);
      assert.equal(r.kind, kind, code);
      assert.equal(r.code, undefined, `${code} is known, so no raw-code suffix`);
      assert.ok(r.text !== code && !CODE.test(r.text) && /^[A-Z]/.test(r.text), `${code} reads as a sentence: ${r.text}`);
    }
  }
  assert.equal(describeServiceReason('starting').text, 'Starting…');
  assert.equal(describeServiceReason('fetching').text, 'Loading…');
  assert.equal(describeServiceReason('no-adapter').text, 'No Bluetooth adapter found.');
});

test('service reasons: every code-shaped literal the Rust services publish has an entry', () => {
  const codes = rustCodes();
  // A broken scan must not pass silently: the Nov 10 sources hold well over 20 codes.
  assert.ok(codes.size >= 20, `scan found only ${codes.size} codes`);
  for (const known of ['no-adapter', 'not-configured', 'scan-failed', 'tailscale-unavailable', 'starting', 'pactl-unavailable', 'checks-unavailable', 'no-private-system-bus']) assert.ok(codes.has(known), `scan missed ${known}`);
  const missing = [...codes].filter(([code]) => !HEALTHY_DETAILS.has(code) && !Object.hasOwn(SERVICE_REASONS, code));
  assert.deepEqual(missing, [], `Rust codes with no user text (add them to ui/island/service-reasons.ts): ${missing.map(([c, f]) => `${c} (${f})`).join(', ')}`);
});

test('service reasons: an unknown code yields the generic sentence and keeps the raw code', () => {
  const r = describeServiceReason('brand-new-failure');
  assert.deepEqual(r, { kind: 'error', text: GENERIC_REASON, code: 'brand-new-failure' });
  const markup = html(ServiceNotice, { icon: 'spark', reason: 'brand-new-failure' });
  assert.match(markup, /class="service-notice-code"> · brand-new-failure</);
  assert.ok(markup.includes(GENERIC_REASON));
  // Known codes never show the suffix.
  assert.doesNotMatch(html(ServiceNotice, { icon: 'download', reason: 'scan-failed' }), /service-notice-code|scan-failed/);
  assert.equal(describeServiceReason('').text, GENERIC_REASON);
});

test('service reasons: healthy backend names are never error text', () => {
  for (const code of HEALTHY) {
    assert.ok(HEALTHY_DETAILS.has(code), code);
    assert.ok(!Object.hasOwn(SERVICE_REASONS, code), `${code} must not have an error entry`);
    const r = describeServiceReason(code);
    assert.notEqual(r.kind, 'error', code);
    assert.equal(r.code, undefined, code);
  }
});

test('service reasons: sentences from the service pass through unchanged, never as a code', () => {
  for (const sentence of ['Timer module disabled', 'Local usage could not be read.', 'AI usage has not been read']) {
    assert.deepEqual(describeServiceReason(sentence), { kind: 'error', text: sentence });
  }
});

test('service reasons: setup buttons point at a real Settings row, and only where one exists', () => {
  const model = readFileSync(join(root, 'ui/views/settings-model.ts'), 'utf8');
  const ids = new Set([...model.matchAll(/\bid: '([A-Za-z0-9]+)'/g)].map(m => m[1]));
  for (const code of Object.keys(SERVICE_REASONS)) {
    const id = SERVICE_REASONS[code].action?.settingId;
    if (id) assert.ok(ids.has(id), `${code} jumps to missing Settings row ${id}`);
  }
  const weather = describeServiceReason('no-location', 'weather');
  assert.equal(weather.kind, 'setup');
  assert.deepEqual(weather.action, { label: 'Choose location', settingId: 'weatherLocation' });
  const markup = html(ServiceNotice, { icon: 'weather', reason: 'no-location', source: 'weather' });
  assert.match(markup, /<span class="service-notice-action"><button type="button" class="settings-cross-link settings-cross-link--text" aria-label="Choose location"/);
  // Downloads come from the desktop's user folders: no Settings row, so no button.
  const downloads = describeServiceReason('not-configured', 'downloads');
  assert.equal(downloads.kind, 'setup');
  assert.equal(downloads.action, undefined);
  assert.doesNotMatch(html(ServiceNotice, { icon: 'download', reason: 'not-configured', source: 'downloads' }), /<button/);
});

test('service reasons: render sites show sentences, not raw codes', () => {
  const card = html(ServiceCard, { title: 'Weather', icon: 'weather', source: 'weather', service: { available: false, reason: 'no-location' }, children: () => null });
  assert.ok(card.includes('Choose a location to see the weather.'), card);
  assert.doesNotMatch(card, /Unavailable ·|no-location/);
  const notice = html(ServiceNotice, { icon: 'download', reason: 'not-configured', source: 'downloads' });
  assert.match(notice, /^<section class="card service-notice" data-kind="setup" role="status"/);
  const timer = html(TimerApp, { snapshot: { rev: 1, available: false, detail: 'stopped', sampledAtMs: 0, completedKind: null, countdownStatus: 'idle', pomodoroStatus: 'idle', stopwatchStatus: 'idle', phase: 'focus', remainingMs: 0, phaseRemainingMs: 0, elapsedMs: 0, round: 0, laps: [] }, completedKind: null, intents: { timer() {} } });
  assert.ok(timer.includes('This service stopped. Restart Monolith to bring it back.'));
  assert.doesNotMatch(timer, /Unavailable ·/);
});
