import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const a = load('ui/views/app-adapters.ts');
const components = Object.fromEntries(['TimerApp','CalendarApp','ScratchpadApp','DownloadsApp','VolumeMixer','NotificationsView','QuickInfo','ControlsApp','AgentRunChips'].map(name => [name, load(`ui/island/components/${name}.tsx`)[name]]));
const { Brightness } = load('ui/island/components/VolumeMixer.tsx');
const render = (name, props) => renderToStaticMarkup(createElement(typeof name === 'string' ? components[name] : name, props));
// Capture the actual returned tree during real SSR, so hooks use React's server
// dispatcher and event assertions exercise production handlers without a DOM mock.
function capture(Component, props) { let tree; function Capture() { tree = Component(props); return tree; } renderToStaticMarkup(createElement(Capture)); return tree; }
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  // Hook-based children must be evaluated under React's dispatcher, not called
  // as plain functions after SSR. Retain the component boundary for intent tests.
  // Class components (framer's PopChildMeasure inside AnimatePresence) cannot be called as
  // functions; keep them as boundaries and walk their children.
  if (typeof tree.type === 'function' && !tree.type.prototype?.isReactComponent) return [tree, ...nodes(capture(tree.type, tree.props))];
  return [tree, ...nodes(tree.props?.children)];
}
const byLabel = (tree, label) => nodes(tree).find(n => n.props?.['aria-label'] === label);
const byText = (tree, label) => nodes(tree).find(n => n.type === 'button' && n.props.children === label);
const intents = new Proxy({}, { get: () => () => {} });
const topic = { rev: 1, available: true, detail: 'ready' };
const timer = (patch = {}) => ({ ...topic, completedKind: null, sampledAtMs: 100, countdownStatus: 'running', remainingMs: 61500, pomodoroStatus: 'idle', phase: 'focus', phaseRemainingMs: 1500000, round: 0, stopwatchStatus: 'idle', elapsedMs: 0, laps: [], ...patch });
const note = (id, pinned = false, updatedMs = 100) => ({ id, title: `Title ${id}`, body: `Body ${id}`, createdMs: 1, updatedMs, pinned });
const scratch = (patch = {}) => ({ ...topic, dirty: true, notes: [note('a'), note('b', true)], ...patch });
const file = (patch = {}) => ({ name: 'report.pdf', sizeBytes: 2048, mtimeMs: 1000, kind: 'document', state: 'ready', ...patch });
const download = (...items) => ({ ...topic, items });
const sink = (patch = {}) => ({ index: 7, name: 'fixture.output', description: 'Fixture speakers', muted: false, volumePercent: null, channels: 2, state: 'RUNNING', ...patch });
const stream = (patch = {}) => ({ index: 42, sinkIndex: 7, appName: 'Fixture player', binary: null, pid: null, mediaName: null, iconName: null, muted: false, corked: true, volumePercent: null, ...patch });
const mixer = (patch = {}) => ({ ...topic, sinks: [sink()], streams: [stream()], apps: [], defaultSink: 'fixture.output', ...patch });
const notification = (patch = {}) => ({ rev: 1, id: 'n1', source: 'system', appName: 'Fixture app', title: 'Fixture notice', body: 'Private body', iconPng: 'data:image/png;base64,aGVsbG8=', urgency: 'normal', receivedAt: 1000, actions: [{ key: 'open', label: 'Private action' }], canInvokeActions: true, progress: null, ...patch });
const calendarPresentation = { monthLabel: 'November 2026', weekdayLabels: ['Mon','Tue','Wed','Thu','Fri','Sat','Sun'], days: [{ key: '2026-11-01', label: '1', inMonth: true, today: true }], view: 'agenda', staleSinceLabel: 'Yesterday' };
const row = (patch = {}) => ({ uid: 'event', summary: 'Fixture event', location: null, startS: 100, endS: 200, allDay: false, source: 'subscription', readOnly: true, ...patch });
const calendar = (patch = {}) => ({ ...topic, configured: true, stale: false, ageMs: 100, updatedAt: 1000, agenda: [row()], ...patch });
const labels = r => ({ instanceKey: `${r.source}:${r.uid}:${r.startS}`, dayKeys: ['2026-11-01'], dateLabel: 'Sunday', timeLabel: '9 AM', spanLabel: null, recurring: null });
const weather = (patch = {}) => ({ rev: 1, available: true, stale: true, tempC: 10, temp: 50, unit: 'fahrenheit', code: 42, icon: null, isDay: null, place: null, updatedAt: null, ...patch });
const power = (patch = {}) => ({ rev: 1, available: true, present: true, percent: null, state: 'pendingCharge', charging: false, onBattery: null, timeToEmpty: null, timeToFull: null, ...patch });
const reasons = { weather: 'No weather source', power: 'No UPower source', bluetooth: null };
const toggles = (patch = {}) => ({ ...topic, powerAvailable: true, powerDetail: 'ready', powerProfile: 'performance', powerChoices: ['balanced','performance'], degraded: null, heldBy: [], micAvailable: true, micDetail: 'ready', micMuted: true, dnd: false, powerRequest: 'power-1', micRequest: 'mic-1', ...patch });
const service = { available: true, detail: 'ready' };
const tiles = (patch = {}) => ({ ...topic, keepAwake: { ...service, on: null, remainingMs: null }, output: { ...service, current: null, choices: [{ name: 'output', description: 'Output', muted: false }] }, network: { ...service, state: 'connectedGlobal', connectivity: 'full', kind: 'wired', showWifi: false, wifiEnabled: null }, tailscale: { ...service, backend: 'futureBackend', selfName: null, onlineCount: null, peers: [{ hostName: 'Offline peer', online: false, os: 'linux', lastSeen: null, firstIp: null }, { hostName: 'Unknown peer', online: null, os: '', lastSeen: null, firstIp: null }, { hostName: 'Online peer', online: true, os: 'linux', lastSeen: null, firstIp: null }] }, voice: { ...service, active: null, choices: ['fixture'] }, pickerAvailable: true, pickerBusy: false, screenshotBusy: false, lastColour: null, actionError: null, ...patch });
const updates = (patch = {}) => ({ ...topic, count: null, repoCount: null, aurCount: null, needsReboot: false, stale: true, error: null, items: [], ageSecs: null, nextCheckInSecs: 60, ...patch });
const chip = id => ({ id, label: `Agent ${id}`, model: null, provider: null, elapsedSecs: 60, apiCallCount: null, isDelegated: true, children: 3 });
const malformed = fn => assert.throws(fn, /Malformed app snapshot/);

test('Timer uses supplied Rust remaining time and never a renderer clock', () => {
  const snapshot = a.adaptTimer(timer());
  const first = render('TimerApp', { snapshot, intents });
  // 61.5 s supplied by Rust → 01:02 (rolling digits are presentational; the label carries it).
  assert.match(first, /role="timer" aria-label="timer running 01:02"/);
  assert.match(first, /class="digit"[^>]*>0<.*class="digit"[^>]*>1<.*class="digit"[^>]*>:<.*class="digit"[^>]*>0<.*class="digit"[^>]*>2</);
  assert.equal(render('TimerApp', { snapshot, intents }), first);
  assert.match(first, /class="timer-start">Pause/);
  assert.match(first, /aria-label="Countdown minutes"[^>]*aria-disabled="true"/, 'the minute ruler is locked while a countdown runs');
});
test('Timer completion dismiss cancels exactly the completed countdown or supplied pomodoro kind', () => {
  const calls = []; const command = { timer: op => calls.push(op) };
  for (const kind of ['countdown','pomodoro']) {
    const snapshot = a.adaptTimer(timer({ countdownStatus: kind === 'countdown' ? 'finished' : 'idle' }));
    const tree = capture(components.TimerApp, { snapshot, intents: command, ...(kind === 'pomodoro' ? { completedKind: kind } : {}) });
    byText(tree, 'Dismiss completed timer').props.onClick();
    assert.deepEqual(calls.at(-1), { type: 'cancel', kind });
  }
});
test('Timer start emits generated setCountdown and start; pause emits only pause', () => {
  const calls = []; const command = { timer: op => calls.push(op) };
  const tree = capture(components.TimerApp, { snapshot: a.adaptTimer(timer({ countdownStatus: 'idle' })), intents: command });
  byText(tree, 'Start').props.onClick();
  assert.deepEqual(calls, [{ type: 'setCountdown', durationMs: 1500000 }, { type: 'start', kind: 'countdown' }]);
  calls.length = 0;
  byText(capture(components.TimerApp, { snapshot: a.adaptTimer(timer()), intents: command }), 'Pause').props.onClick();
  assert.deepEqual(calls, [{ type: 'pause', kind: 'countdown' }]);
  calls.length = 0;
  byText(capture(components.TimerApp, { snapshot: a.adaptTimer(timer({ countdownStatus: 'finished' })), intents: command }), 'Start').props.onClick();
  assert.deepEqual(calls, [{ type: 'start', kind: 'countdown' }], 'SetCountdown is invalid while finished');
});
test('Timer rejects invented statuses, phases, missing fields, malformed numbers and oversized laps', () => {
  for (const patch of [{ pomodoroStatus: 'finished' }, { countdownStatus: 'busy' }, { stopwatchStatus: 'finished' }, { phase: 'break' }, { remainingMs: -1 }, { elapsedMs: NaN }, { sampledAtMs: Infinity }, { round: 0.1 }, { laps: Array(1001).fill({ number: 1, elapsedMs: 0 }) }]) malformed(() => a.adaptTimer(timer(patch)));
  for (const raw of [null, [], {}, 'timer']) malformed(() => a.adaptTimer(raw));
});
test('Calendar keeps host labels, exclusive day expansion, instance identity and unknown recurrence', () => {
  const model = a.adaptCalendar(calendar(), calendarPresentation, labels);
  assert.equal(model.agenda[0].recurring, null);
  assert.deepEqual(model.agenda[0].dayKeys, ['2026-11-01']);
  const html = render('CalendarApp', { model, intents });
  assert.match(html, /9 AM/); assert.match(html, /Sunday/); assert.match(html, /Subscription · Read-only/); assert.doesNotMatch(html, /Recurring/);
});
test('Calendar projects out secret URL and fetch details; setup exposes public provider instructions only', () => {
  const secret = 'https://secret.invalid/private-token.ics';
  const model = a.adaptCalendar(calendar({ configured: false, available: false, stale: true, detail: 'fetch-failed', subscriptionUrl: secret }), calendarPresentation, labels);
  const calls = []; const tree = capture(components.CalendarApp, { model, intents: { calendar: op => calls.push(op) } });
  byText(tree, 'Set up subscription').props.onClick();
  assert.deepEqual(calls, [{ type: 'configure' }]);
  const html = render('CalendarApp', { model, intents });
  assert.match(html, /Fetch failed/); assert.match(html, /Yesterday/);
  assert.doesNotMatch(html, /secret\.invalid|private-token|fetch-failed/);
  assert.match(html, /iCal \(ICS\) subscription/);
  byText(tree, 'Refresh').props.onClick(); assert.deepEqual(calls.at(-1), { type: 'refresh' });
});
test('Calendar select and month view preserve supplied identity and day placement', () => {
  const model = a.adaptCalendar(calendar(), { ...calendarPresentation, view: 'month' }, labels);
  const calls = []; const tree = capture(components.CalendarApp, { model, intents: { calendar: op => calls.push(op) } });
  nodes(tree).find(n => n.type === 'button' && n.props.className?.startsWith('calendar-event')).props.onClick();
  assert.deepEqual(calls, [{ type: 'select', instanceKey: 'subscription:event:100' }]);
  assert.match(render('CalendarApp', { model, intents }), /month-day/);
});
test('Calendar validates source/readOnly, time windows, recurrence, labels and duplicated instances', () => {
  for (const patch of [{ source: 'google' }, { readOnly: false }, { endS: 99 }, { summary: 'x'.repeat(8193) }]) malformed(() => a.adaptCalendar(calendar({ agenda: [row(patch)] }), calendarPresentation, labels));
  malformed(() => a.adaptCalendar(calendar(), calendarPresentation, r => ({ ...labels(r), recurring: 'unknown' })));
  malformed(() => a.adaptCalendar(calendar(), { ...calendarPresentation, view: 'year' }, labels));
  malformed(() => a.adaptCalendar(calendar({ agenda: [row(),row()] }), calendarPresentation, labels));
  malformed(() => a.adaptCalendar(calendar(), { ...calendarPresentation, weekdayLabels: [] }, labels));
  malformed(() => a.adaptCalendar(calendar(), calendarPresentation, r => ({ ...labels(r), dayKeys: ['2026-02-31'] })));
});
test('Scratchpad saving and save-failed error are Rust-props driven, selection remains local', () => {
  const model = a.adaptScratchpad(scratch(), 'a');
  assert.equal(model.selectedId, 'a'); assert.equal(model.save.status, 'saving');
  const html = render('ScratchpadApp', { model, intents });
  assert.match(html, /Saving…/); assert.match(html, />Body a<\/textarea>/); assert.ok(html.indexOf('Title b') < html.indexOf('Title a'));
  assert.doesNotMatch(html, /1970|January/);
  const failed = a.adaptScratchpad(scratch({ available: false, dirty: false, detail: 'save-failed' }), 'b');
  assert.equal(failed.save.status, 'error'); assert.match(render('ScratchpadApp', { model: failed, intents }), /Save failed/);
  assert.equal(a.adaptScratchpad(scratch({ dirty: false }), 'missing').selectedId, null);
});
test('Scratchpad edits and pin/delete dispatch generated identities without allocating timestamps', () => {
  const calls = []; const model = a.adaptScratchpad(scratch(), 'a');
  const tree = capture(components.ScratchpadApp, { model, intents: { scratchpad: op => calls.push(op) } });
  nodes(tree).find(n => n.type === 'textarea').props.onChange({ target: { value: 'New body' } });
  byText(tree, 'Pin note').props.onClick(); byText(tree, 'Delete note').props.onClick(); byText(tree, 'New note').props.onClick();
  assert.deepEqual(calls, [{ type: 'edit', id: 'a', body: 'New body' }, { type: 'pin', id: 'a', pinned: true }, { type: 'delete', id: 'a' }, { type: 'create' }]);
});
test('Scratchpad rejects invalid IDs, oversized UTF-8 bodies and nonmonotonic timestamps', () => {
  for (const patch of [{ id: '../note' }, { id: 'x'.repeat(65) }, { body: 'é'.repeat(131073) }, { createdMs: 200, updatedMs: 100 }, { updatedMs: -1 }, { pinned: null }]) malformed(() => a.adaptScratchpad(scratch({ notes: [{ ...note('a'), ...patch }] }), 'a'));
  assert.equal(a.adaptScratchpad(scratch({ notes: [{ ...note('a'), body: 'é'.repeat(131072) }] }), 'a').notes[0].body.length, 131072);
});
test('Downloads sort newest first and never open partial filenames even in ready state', () => {
  const model = a.adaptDownloads(download(file({ name: 'old.pdf', mtimeMs: 1 }), file({ name: 'new.pdf.part', mtimeMs: 2 })), () => 'Now');
  const html = render('DownloadsApp', { items: model.value, intents });
  assert.ok(html.indexOf('new.pdf.part') < html.indexOf('old.pdf')); assert.doesNotMatch(html, /aria-label="Open new.pdf.part"/); assert.match(html, /Open old.pdf/);
  const calls = []; const tree = capture(components.DownloadsApp, { items: model.value, intents: { download: op => calls.push(op) } });
  byLabel(tree, 'Reveal new.pdf.part').props.onClick(); assert.deepEqual(calls, [{ type: 'reveal', name: 'new.pdf.part' }]);
});
test('Downloads refuse traversal, malformed states/kinds/sizes and oversized filenames', () => {
  for (const patch of [{ name: '../file' }, { name: 'dir\\file' }, { name: '..' }, { name: 'é'.repeat(128) }, { state: 'complete' }, { kind: 'exe' }, { sizeBytes: -1 }, { sizeBytes: 0.5 }, { sizeBytes: Number.MAX_SAFE_INTEGER + 1 }, { mtimeMs: NaN }]) malformed(() => a.adaptDownloads(download(file(patch))));
  assert.equal(a.adaptDownloads(download(file({ name: 'x'.repeat(255) }))).available, true);
});
test('Downloads retain unavailable reasons instead of inventing an empty successful service', () => {
  assert.deepEqual(a.adaptDownloads({ ...download(), available: false, detail: 'No download directory' }), { available: false, reason: 'No download directory' });
});
test('Mixer preserves null volume, real sink/stream identities and observed boosts', () => {
  const model = a.adaptMixer(mixer()); const html = render('VolumeMixer', { model, intents });
  assert.equal(model.value.sinks[0].index, 7); assert.equal(model.value.streams[0].index, 42);
  // Unknown volume: no fader at all, an explicit placeholder instead of a fabricated 0 %.
  assert.match(html, /<p class="muted"[^>]*>Unknown<\/p>/); assert.doesNotMatch(html, /role="slider"/); assert.match(html, /fader-pct">—</);
  const boosted = render('VolumeMixer', { model: a.adaptMixer(mixer({ sinks: [sink({ volumePercent: 150 })] })), intents });
  assert.match(boosted, /fader-pct">150%</, 'the observed boost is shown, not clamped');
  assert.match(boosted, /aria-valuenow="100"/, 'the fader itself tops out at 100 %');
  assert.equal(a.adaptMixer(mixer({ sinks: [sink({ state: '' })] })).value.sinks[0].state, '', 'Core leaves absent sink state empty, not a fabricated running state');
});
test('Mixer controls emit sink names rather than invented sink indices', () => {
  const calls = []; const model = a.adaptMixer(mixer({ sinks: [sink({ volumePercent: 50 })], streams: [stream({ volumePercent: 20 })] }));
  const tree = capture(components.VolumeMixer, { model, intents: { mixer: op => calls.push(op) } });
  nodes(tree).find(n => typeof n.type === 'function' && n.props?.label === 'Output' && n.props.onCommit).props.onCommit(90);
  byLabel(tree, 'Mute Fixture player').props.onClick();
  nodes(tree).find(n => typeof n.type === 'function' && n.props?.label === 'Mixer output').props.onChange('fixture.output');
  assert.deepEqual(calls, [{ type: 'sinkVolume', name: 'fixture.output', percent: 90 }, { type: 'streamMute', index: 42, muted: true }, { type: 'defaultSink', name: 'fixture.output' }]);
});
test('Mixer and brightness reject malformed identities, enums, null mute and range errors', () => {
  for (const patch of [{ index: -1 }, { name: '-bad' }, { name: 'bad output' }, { name: 'x'.repeat(257) }, { muted: null }, { state: 'exploding' }, { volumePercent: -1 }]) malformed(() => a.adaptMixer(mixer({ sinks: [sink(patch)] })));
  malformed(() => a.adaptMixer(mixer({ streams: [stream({ sinkIndex: null })] })));
  malformed(() => a.adaptBrightness({ ...topic, displays: [{ name: 'DP-1', label: 'Monitor', isInternal: false, percent: 101, dragging: false }] }));
});
test('Brightness emits generated drag/release operations and unknown value has no slider', () => {
  const displays = a.adaptBrightness({ ...topic, displays: [{ name: 'DP-1', label: 'Monitor', isInternal: false, percent: 60, dragging: true }, { name: 'DP-2', label: 'Other', isInternal: false, percent: null, dragging: false }] });
  const html = render(Brightness, { displays, intents }); assert.match(html, /Brightness unknown/); assert.equal([...html.matchAll(/type="range"/g)].length, 1);
  const calls = []; const tree = capture(Brightness, { displays, intents: { brightness: op => calls.push(op) } });
  // DraftRange commits throttled drafts while dragging and a final value on release.
  const slider = byLabel(tree, 'Monitor brightness'); slider.props.onCommit(70, false); slider.props.onCommit(80, true);
  assert.deepEqual(calls, [{ type: 'drag', name: 'DP-1', percent: 70 }, { type: 'drag', name: 'DP-1', percent: 80 }, { type: 'release', name: 'DP-1' }]);
});
test('Notifications are passive regardless of wire action capability and DND is supplied', () => {
  const model = a.adaptNotifications([notification()], true, false);
  assert.equal(model.items[0].canInvokeActions, false);
  const html = render('NotificationsView', { model, intents }); assert.match(html, /Do Not Disturb/); assert.match(html, /<button disabled="" title="Passive notification mirror">Private action/);
  const tree = capture(components.NotificationsView, { model, intents }); assert.equal(byText(tree, 'Private action').props.onClick, undefined);
});
test('Notification redaction scrubs body, icon and actions at both adapter and view boundaries', () => {
  const model = a.adaptNotifications([notification()], false, true);
  assert.equal(model.items[0].body, null); assert.equal(model.items[0].iconPng, null); assert.deepEqual(model.items[0].actions, []);
  const direct = { ...model, items: [{ ...notification(), hidden: true, ageLabel: 'Now' }] };
  const html = render('NotificationsView', { model: direct, intents });
  assert.match(html, /Content hidden/); assert.doesNotMatch(html, /Private body|Private action|data:image|<img/);
});
test('Notification grouping sorts newest first, dismiss uses exact id and empty clear is disabled', () => {
  const model = a.adaptNotifications([notification({ id: 'old', title: 'Old', receivedAt: 1 }), notification({ id: 'new', title: 'New', receivedAt: 2 })], false, false);
  const html = render('NotificationsView', { model, intents }); assert.ok(html.indexOf('>New<') < html.indexOf('>Old<'));
  const calls = []; const tree = capture(components.NotificationsView, { model, intents: { notification: op => calls.push(op) } });
  byLabel(tree, 'Dismiss New').props.onClick(); byText(tree, 'Clear all').props.onClick(); assert.deepEqual(calls, [{ type: 'dismiss', id: 'new' }, { type: 'clearAll' }]);
  assert.match(render('NotificationsView', { model: a.adaptNotifications([], false, false), intents }), /<button disabled="">Clear all/);
});
test('Notifications reject invalid urgency/source/progress, unsafe image URLs and oversize text', () => {
  for (const patch of [{ urgency: 'urgent' }, { source: 'dbus' }, { progress: 2 }, { iconPng: 'file:///secret' }, { title: 'x'.repeat(4097) }, { body: 'x'.repeat(16385) }]) malformed(() => a.adaptNotifications([notification(patch)], false, false));
});
test('QuickInfo uses selected Fahrenheit value, generic WMO and exact nullable power state', () => {
  const model = a.adaptQuickInfo(weather(), power(), [], reasons);
  const html = render('QuickInfo', { model }); assert.match(html, /50°F/); assert.doesNotMatch(html, /10°C|0%|battery alert/); assert.match(html, /WMO 42/); assert.match(html, /Unknown charge/); assert.match(html, /Pending charge/);
  assert.equal(model.power.value.onBattery, null);
  assert.match(render('QuickInfo', { model: a.adaptQuickInfo(weather({ unit: 'celsius', temp: 10, code: 97 }), power({ state: 'fullyCharged', percent: 100 }), [], reasons) }), /10°C/);
});
test('QuickInfo never fabricates missing values, alerts, paired state or service reasons', () => {
  const model = a.adaptQuickInfo(weather({ temp: null }), power({ present: false }), [{ address: 'AA', name: 'Headset', path: '/fixture/headset', icon: null, connected: false, paired: true, batteryPercent: null }], reasons);
  const html = render('QuickInfo', { model }); assert.match(html, /Unknown temperature/); assert.match(html, /No battery present/); assert.match(html, /Paired · Disconnected/); assert.match(html, /Battery unknown/);
  const unavailable = render('QuickInfo', { model: a.adaptQuickInfo(null, null, [], { ...reasons, bluetooth: 'No BlueZ' }) });
  for (const reason of ['No weather source','No UPower source','No BlueZ']) assert.ok(unavailable.includes(reason));
});
test('QuickInfo rejects malformed weather/power enums and battery values', () => {
  for (const patch of [{ unit: 'kelvin' }, { isDay: 0 }, { temp: Infinity }, { code: -1 }, { place: { name: 'X', country: 'Y', latitude: 100, longitude: 0 } }]) malformed(() => a.adaptQuickInfo(weather(patch), power(), [], reasons));
  for (const patch of [{ state: 'Unknown' }, { percent: 101 }, { charging: null }]) malformed(() => a.adaptQuickInfo(weather(), power(patch), [], reasons));
});
test('Controls preserve request IDs and optimistic display without fabricated confirmed/pending DTOs', () => {
  const model = a.adaptControls(toggles(), tiles(), updates());
  assert.equal(model.toggles.micRequest, 'mic-1'); assert.equal(model.toggles.powerRequest, 'power-1');
  assert.equal(model.tiles.keepAwake.on, null); assert.equal(model.tiles.output.current, null); assert.equal(model.updates.count, null);
  const html = render('ControlsApp', { model, intents }); assert.match(html, /Pending · performance/); assert.match(html, /Pending · On/); assert.match(html, /aria-busy="true"/); assert.match(html, /Unknown · no successful check/);
});
test('Controls preserve offline false separately from null and do not invent online counts', () => {
  const model = a.adaptControls(toggles({ micRequest: null, powerRequest: null }), tiles(), updates());
  assert.equal(model.tiles.tailscale.peers[0].online, false); assert.equal(model.tiles.tailscale.peers[1].online, null);
  const html = render('ControlsApp', { model, intents }); assert.match(html, /Offline · Last seen: Unknown/); assert.match(html, /Online status unknown/); assert.match(html, /Unknown online/); assert.ok(html.indexOf('>Online peer<') < html.indexOf('>Offline peer<')); assert.doesNotMatch(html, /Pending ·/);
});
test('Controls emit exact generated toggle/tile/update ops and disable busy picker', () => {
  const model = a.adaptControls(toggles(), tiles({ pickerBusy: true }), updates());
  const calls = []; const command = { toggle: op => calls.push(op), tile: op => calls.push(op), updates: op => calls.push(op) };
  const tree = capture(components.ControlsApp, { model, intents: command });
  byLabel(tree, 'Mic mute').props.onClick(); byLabel(tree, 'Cycle power profile').props.onClick();
  const awake = nodes(tree).find(n => typeof n.type === 'function' && n.props.label === 'Keep awake duration');
  assert.ok(awake.props.options.some(o => o.value === 'off'));
  awake.props.onChange('off'); byText(tree, 'Check updates').props.onClick();
  assert.deepEqual(calls, [{ type: 'micMute', muted: false }, { type: 'cycleProfile' }, { type: 'keepAwake', on: false, durationMs: null }, { type: 'refresh' }]);
  const picker = nodes(tree).find(n => n.type === 'button' && nodes(n).some(n => n.type === 'strong' && n.props.children === 'Colour picker')); assert.equal(picker.props.disabled, true);
});
test('Controls reject malformed closed enums and oversized request IDs, preserving opaque backend names', () => {
  for (const patch of [{ powerProfile: 'turbo' }, { powerRequest: 1 }, { micRequest: 'x'.repeat(129) }, { micMuted: 'true' }]) malformed(() => a.adaptControls(toggles(patch), tiles(), updates()));
  malformed(() => a.adaptControls(toggles(), tiles({ network: { ...tiles().network, state: 'online' } }), updates()));
  malformed(() => a.adaptControls(toggles(), tiles(), updates({ items: [{ name: 'pkg', oldVersion: '1', newVersion: '2', ignored: false, source: 'mystery' }] })));
  assert.equal(a.adaptControls(toggles(), tiles(), updates()).tiles.tailscale.backend, 'futureBackend');
});
test('Agent chips consume all core-supplied chips and exact overflow without recapping', () => {
  const model = a.adaptAgents({ ...topic, chips: Array.from({ length: 8 }, (_, i) => chip(String(i))), overflow: 9 });
  const html = render('AgentRunChips', { update: model.value, intents }); assert.equal([...html.matchAll(/class="agent-chip"/g)].length, 8); assert.match(html, /Show 9 more agent runs/); assert.match(html, />\+9</); assert.match(html, /3 children/); assert.match(html, /Unknown model/);
  const calls = []; const tree = capture(components.AgentRunChips, { update: model.value, intents: { agent: id => calls.push(id) } });
  nodes(tree).find(n => n.type === 'button' && n.props.className === 'agent-chip').props.onClick(); byLabel(tree, 'Show 9 more agent runs').props.onClick(); assert.deepEqual(calls, ['0',null]);
});
test('Agent adapter rejects malformed counts, duplicate identities and oversize labels', () => {
  for (const patch of [{ overflow: -1 }, { chips: [chip('a'),chip('a')] }, { chips: [{ ...chip('a'), children: 1.5 }] }, { chips: [{ ...chip('a'), label: 'x'.repeat(257) }] }]) malformed(() => a.adaptAgents({ ...topic, chips: [], overflow: 0, ...patch }));
});
test('All adapters fail closed on wrong root JSON rather than casting arbitrary values', () => {
  const adapters = [a.adaptTimer, x => a.adaptCalendar(x, calendarPresentation, labels), x => a.adaptScratchpad(x, null), a.adaptDownloads, a.adaptMixer, a.adaptBrightness, a.adaptAgents, x => a.adaptControls(x, tiles(), updates())];
  for (const adapter of adapters) for (const raw of [null, [], true, 42, 'snapshot', {}]) malformed(() => adapter(raw));
});
