import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';

// Phase 6a page polish: Notifications grouping, Controls tiles/Tailscale summary, AI empty state, Now playing names.
const render = (C, props) => renderToStaticMarkup(createElement(C, props));
function capture(Component, props) { let tree; function Capture() { tree = Component(props); return tree; } renderToStaticMarkup(createElement(Capture)); return tree; }
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (typeof tree.type === 'function' && !tree.type.prototype?.isReactComponent) return [tree, ...nodes(capture(tree.type, tree.props))];
  return [tree, ...nodes(tree.props?.children)];
}
const byLabel = (tree, label) => nodes(tree).find(n => n.props?.['aria-label'] === label);

// ---- Notifications ----
const { groupNotifications } = load('ui/island/notification-groups.ts');
const a = load('ui/views/app-adapters.ts');
const { NotificationsView } = load('ui/island/components/NotificationsView.tsx');
const wire = (id, patch = {}) => ({ rev: 1, id, source: 'hermes', appName: 'Agents', title: 'Agent run finished', body: null, iconPng: null, urgency: 'normal', receivedAt: 1000, actions: [], canInvokeActions: false, progress: null, ...patch });
const ids = rows => rows.flatMap(r => r.ids);

test('grouping: per app, newest first, identical repeats fold into one counted row and every id stays reachable', () => {
  const model = a.adaptNotifications([
    wire('s1', { title: 'Agent run started', receivedAt: 1 }), wire('f1', { receivedAt: 2 }), wire('s2', { title: 'Agent run started', receivedAt: 3 }),
    wire('s3', { title: 'Agent run started', receivedAt: 5 }), wire('m1', { appName: 'Mail', title: 'New mail', source: 'system', receivedAt: 4 }),
  ], false, false, at => `t${at}`);
  const groups = groupNotifications(model.items);
  assert.deepEqual(groups.map(g => [g.name, g.count]), [['Agents', 4], ['Mail', 1]]);
  const [agents] = groups;
  assert.deepEqual(agents.rows.map(r => [r.item.title, r.count, r.item.ageLabel]), [['Agent run started', 3, 't5'], ['Agent run finished', 1, 't2']]);
  assert.deepEqual(agents.rows[0].ids, ['s3', 's2', 's1']);
  assert.deepEqual(ids(groups.flatMap(g => g.rows)).sort(), ['f1', 'm1', 's1', 's2', 's3']);
});

test('grouping never folds approval-like rows: actions, critical urgency, progress, or a different body or app', () => {
  const items = a.adaptNotifications([
    wire('a1', { actions: [{ key: 'allow', label: 'Allow' }], receivedAt: 1 }), wire('a2', { actions: [{ key: 'allow', label: 'Allow' }], receivedAt: 2 }),
    wire('c1', { urgency: 'critical', receivedAt: 3 }), wire('c2', { urgency: 'critical', receivedAt: 4 }),
    wire('p1', { progress: 0.5, receivedAt: 5 }), wire('p2', { progress: 0.5, receivedAt: 6 }),
    wire('b1', { body: 'one', receivedAt: 7 }), wire('b2', { body: 'two', receivedAt: 8 }), wire('x1', { appName: 'Other', receivedAt: 9 }),
  ], false, false).items;
  const groups = groupNotifications(items);
  assert.equal(groups.flatMap(g => g.rows).length, 9);
  assert.ok(groups.flatMap(g => g.rows).every(r => r.count === 1 && r.ids.length === 1));
});

test('grouping is a view: the input list is not reordered, dropped or rewritten', () => {
  const items = a.adaptNotifications([wire('n1', { receivedAt: 1 }), wire('n2', { receivedAt: 2 })], false, false).items;
  const before = JSON.stringify(items);
  groupNotifications(items);
  assert.equal(JSON.stringify(items), before);
});

const view = items => ({ dnd: false, items: a.adaptNotifications(items, false, false, at => `${at} min ago`).items });
test('view: 14 repeats render as one group header with a count and two compact rows', () => {
  const raw = Array.from({ length: 14 }, (_, i) => wire(`n${i}`, { title: i % 3 === 2 ? 'Agent run finished' : 'Agent run started', receivedAt: i + 1 }));
  const html = render(NotificationsView, { model: view(raw), intents: { notification() {} } });
  assert.equal([...html.matchAll(/class="notification-row"/g)].length, 2);
  assert.match(html, /<h3>.*?Agents<\/h3><span class="notification-count muted">14<\/span>/);
  assert.match(html, /14 notifications/);
  assert.match(html, /×10/); assert.match(html, /×4/);
  assert.match(html, /14 min ago/); assert.match(html, /12 min ago/);
  assert.doesNotMatch(html, /Age unavailable/);
});

test('view: dismissing a folded row or a group issues exactly the per-id dismiss each row issues today', () => {
  const raw = [wire('a', { receivedAt: 1 }), wire('b', { receivedAt: 2 }), wire('c', { title: 'Other', receivedAt: 3 })];
  const calls = []; const tree = capture(NotificationsView, { model: view(raw), intents: { notification: op => calls.push(op) } });
  byLabel(tree, 'Dismiss Agent run finished (2)').props.onClick();
  assert.deepEqual(calls.splice(0), [{ type: 'dismiss', id: 'b' }, { type: 'dismiss', id: 'a' }]);
  byLabel(tree, 'Dismiss Other').props.onClick();
  assert.deepEqual(calls.splice(0), [{ type: 'dismiss', id: 'c' }]);
  byLabel(tree, 'Dismiss all Agents notifications').props.onClick();
  assert.deepEqual(calls.splice(0).map(o => o.id).sort(), ['a', 'b', 'c']);
  assert.ok(calls.length === 0);
});

test('view: a single-notification group has no group dismiss; clear all stays; redacted rows leak nothing', () => {
  const one = capture(NotificationsView, { model: view([wire('only')]), intents: { notification() {} } });
  assert.equal(byLabel(one, 'Dismiss all Agents notifications'), undefined);
  const hidden = { dnd: false, items: a.adaptNotifications([wire('h1', { body: 'secret', iconPng: 'data:image/png;base64,aGVsbG8=' })], false, true).items };
  const html = render(NotificationsView, { model: hidden, intents: { notification() {} } });
  assert.match(html, /Content hidden/); assert.doesNotMatch(html, /secret|data:image|<img/);
});

// ---- Controls ----
const { ControlsApp } = load('ui/island/components/ControlsApp.tsx');
const toggles = (patch = {}) => ({ available: true, detail: 'ready', dnd: false, micAvailable: true, micDetail: 'ready', micMuted: false, micRequest: null, powerAvailable: true, powerDetail: 'ready', powerProfile: 'balanced', powerChoices: ['power-saver', 'balanced', 'performance'], powerRequest: null, degraded: null, heldBy: [], ...patch });
const peers = [{ hostName: 'alpha', os: 'linux', online: true, lastSeen: null }, { hostName: 'beta', os: 'android', online: false, lastSeen: '2026-08-09T06:39:51.1Z' }];
const tiles = (patch = {}) => ({
  output: { available: false, detail: 'Unavailable' }, network: { available: true, kind: 'wifi', state: 'connected', connectivity: 'full' },
  tailscale: { available: true, backend: 'Running', selfName: 'desk', onlineCount: 1, peers, ...patch }, voice: { available: false, detail: 'Unavailable' },
  keepAwake: { available: true, detail: 'ready', on: false, remainingMs: null },
});
const controls = (t = toggles(), s = tiles()) => ({ toggles: t, tiles: s, updates: null });

test('controls: DND, mic and power share one tile row ahead of the Caffeine/system cards, with the same labels', () => {
  const html = render(ControlsApp, { model: controls(), intents: {} });
  const row = html.slice(html.indexOf('controls-tiles'), html.indexOf('controls-grid'));
  assert.match(row, /aria-label="Do Not Disturb"/); assert.match(row, /aria-label="Mic mute"/); assert.match(row, /aria-label="Cycle power profile"/);
  assert.ok(html.indexOf('controls-tiles') < html.indexOf('controls-grid'));
  assert.doesNotMatch(html.slice(html.indexOf('controls-grid')), /aria-label="(Do Not Disturb|Mic mute|Cycle power profile)"/);
});

test('controls: the power tile shows one pip per profile with the current one lit and still cycles on click', () => {
  const html = render(ControlsApp, { model: controls(), intents: {} });
  const steps = html.match(/<span class="power-steps"[^>]*>(.*?)<\/span>/)[1];
  assert.deepEqual([...steps.matchAll(/data-on="(true|false)"/g)].map(m => m[1]), ['false', 'true', 'false']);
  const calls = []; const tree = capture(ControlsApp, { model: controls(), intents: { toggle: op => calls.push(op), tile() {} } });
  byLabel(tree, 'Cycle power profile').props.onClick();
  assert.deepEqual(calls, [{ type: 'cycleProfile' }]);
});

test('controls: Tailscale is a native collapsed-by-default details with a one-line summary and every peer inside', () => {
  const html = render(ControlsApp, { model: controls(), intents: {} });
  assert.match(html, /<details class="tailscale-details"><summary><span>Tailscale · Running · desk · 1 online<\/span><\/summary>/);
  assert.doesNotMatch(html, /<details[^>]*\bopen\b/);
  const body = html.slice(html.indexOf('<ul class="peer-list">'), html.indexOf('</details>'));
  assert.match(body, /alpha/); assert.match(body, /beta/); assert.match(body, /Offline · Last seen:/);
  assert.ok(body.indexOf('alpha') < body.indexOf('beta'));
});

test('controls: collapse state lives in the DOM only, never in props or settings', () => {
  const source = readFileSync(new URL('../ui/island/components/ControlsApp.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /useState|useEffect|localStorage|settings/i);
  const src = renderToStaticMarkup(createElement(ControlsApp, { model: controls(undefined, tiles({ available: false, detail: 'tailscaled stopped' })), intents: {} }));
  assert.doesNotMatch(src, /<details/); assert.match(src, /tailscaled stopped/);
});

// ---- AI page, Live tab ----
const live = load('ui/island/components/AgentLive.tsx');
const model = load('ui/views/agent-live-model.ts');
const liveNow = 1_800_000_000_000;
const emptyInput = (patch = {}) => ({ sessionId: null, matchedBy: 'none', snapshot: null, snapshotError: 'n/a', plan: null, planError: 'n/a', db: null, dbError: 'x', log: null, logError: 'agent.log not found', history: null, blocks: [], ...patch });
const emptyView = patch => JSON.parse(JSON.stringify(model.buildView(emptyInput(patch), { now: liveNow, ...model.VIEW_DEFAULTS, page: 'live' })));
const cardCount = html => [...html.matchAll(/<section class="ai-card/g)].length;

test('AI Live with no session: one empty-state card, none of the Unavailable/dash cards, sources footnote kept', () => {
  const view = emptyView();
  assert.equal(live.hasLiveSession(view), false);
  const html = render(live.AgentLivePage, { view, now: liveNow, onCopy() {} });
  assert.equal(cardCount(html), 1);
  assert.match(html, /No live session/); assert.match(html, /No live Hermes session found/); assert.match(html, /log agent.log not found/);
  assert.doesNotMatch(html, /Unavailable|No turn yet|No activity|>—<|NaN|undefined/);
  for (const gone of ['Activity', 'Context', 'Cache', 'This turn', '>Session<', '>Agents<']) assert.ok(!html.includes(gone), `still renders ${gone}`);
});

test('AI Live empty state mentions a missing plugin once, and the extra cards (Claude Code) still render after it', () => {
  const view = emptyView(); view.sources.plugin = 'not installed';
  const html = render(live.AgentLivePage, { view, now: liveNow, onCopy() {}, extra: createElement('section', { className: 'ai-card', id: 'extra' }, 'Claude extra') });
  assert.equal(cardCount(html), 2);
  assert.equal([...html.matchAll(/habby-metrics/g)].length >= 1, true);
  assert.doesNotMatch(html, /al-plugin-empty/);
  assert.ok(html.indexOf('al-empty') < html.indexOf('Claude extra'));
});

test('AI Live with a resolved session keeps the full card set (no empty-state card)', () => {
  const view = emptyView(); view.who = { id: 'S_ONE', short: 'S_ONE', matchedBy: 'db-newest' };
  assert.equal(live.hasLiveSession(view), true);
  const html = render(live.AgentLivePage, { view, now: liveNow, onCopy() {} });
  assert.doesNotMatch(html, /al-empty/); assert.ok(cardCount(html) >= 6);
  const viaAgents = emptyView(); viaAgents.agents = { main: { id: 'M', status: 'idle' }, subs: [], earlier: 0 };
  assert.equal(live.hasLiveSession(viaAgents), true);
});

test('AI Live empty state surfaces a real state.db problem but not the benign "no live session"', () => {
  const view = emptyView(); view.sources.db = 'db busy';
  assert.match(render(live.AgentLivePage, { view, now: liveNow, onCopy() {} }), /No live Hermes session found \(state\.db: db busy\)/);
  const benign = emptyView(); benign.sources.db = 'no live session';
  const html = render(live.AgentLivePage, { view: benign, now: liveNow, onCopy() {} });
  assert.match(html, /<p class="al-empty-title">No live Hermes session found<\/p>/);
});

// ---- Now playing ----
const { friendlyPlayerName } = load('ui/island/player-name.ts');
const np = load('ui/island/components/NowPlaying.tsx');
const track = (patch = {}) => ({ player: 'plasma-browser-integration', title: 'Song', artist: 'Artist', album: 'Album', playing: true, positionMs: 1000, durationMs: 180000, artUrl: null, canControl: true, busName: 'org.mpris.MediaPlayer2.plasma-browser-integration', status: 'playing', rate: 1, sampledAtMs: 1000, trackId: '/t', canSeek: true, canGoNext: true, canGoPrevious: true, ...patch });
const playing = (lyrics, trackPatch) => ({ nowPlaying: track(trackPatch), lyrics: { lines: [], plain: null, activeIndex: null, ...lyrics }, equalizer: { active: false, bands: [] } });

test('player names: known ids map, instance suffixes and reverse-DNS prefixes drop, unknown ids title-case', () => {
  const cases = [
    ['plasma-browser-integration', 'Browser'], ['spotify', 'Spotify'], ['firefox.instance4242', 'Firefox'], ['chromium.instance1', 'Chromium'], ['vlc', 'VLC'],
    ['org.kde.haruna', 'Haruna'], ['my-cool_player', 'My Cool Player'], ['org.example.some-new-app', 'Some New App'], ['SPOTIFY', 'Spotify'], ['  ', 'Player'], ['', 'Player'],
  ];
  for (const [raw, shown] of cases) assert.equal(friendlyPlayerName(raw), shown, raw);
  for (const raw of ['plasma-browser-integration', 'my-cool_player', 'x.instance9']) assert.doesNotMatch(friendlyPlayerName(raw), /[-_]|instance\d/);
});

test('Now playing shows the friendly name, never the raw identifier, and keeps the raw one for stream matching', () => {
  const model = playing({ status: 'off' });
  const html = render(np.NowPlaying, { model, intents: { media() {} }, lyricsEnabled: false });
  assert.match(html, /<small class="muted">Browser<\/small>/); assert.doesNotMatch(html, /plasma-browser-integration/);
  assert.equal(np.playerStream(model.nowPlaying, [{ index: 1, appName: 'plasma-browser-integration', binary: null, pid: null }]).index, 1);
});

test('Now playing: no lyrics collapses the half-page stage to a one-line note, with Turn off still reachable', () => {
  for (const [status, note] of [['notFound', /No lyrics found for this track/], ['idle', /No lyrics found/], ['instrumental', /Instrumental/], ['loading', /Loading lyrics/], ['error', /Lyrics lookup failed/]]) {
    const calls = []; const model = playing({ status });
    const html = render(np.NowPlaying, { model, intents: { media() {} }, lyricsEnabled: true });
    assert.match(html, /data-lyrics="false"/, status); assert.doesNotMatch(html, /np-lyrics"|np-backdrop|np-lyrics-scroll/, status);
    assert.match(html, note, status); assert.match(html, /np-lyrics-line/, status);
    const tree = capture(np.NowPlaying, { model, intents: { media() {} }, lyricsEnabled: true, onLyricsEnabled: v => calls.push(v) });
    nodes(tree).find(n => n.type === 'button' && n.props.children === 'Turn off').props.onClick();
    assert.deepEqual(calls, [false]);
  }
});

test('Now playing: lyrics (synced or plain) and the opt-in prompt keep the full lyrics stage', () => {
  const synced = render(np.NowPlaying, { model: playing({ status: 'synced', lines: [{ timeMs: 0, text: 'one' }], activeIndex: 0 }), intents: { media() {} }, lyricsEnabled: true });
  assert.match(synced, /data-lyrics="true"/); assert.match(synced, /np-lyrics/); assert.doesNotMatch(synced, /np-lyrics-line/);
  assert.match(render(np.NowPlaying, { model: playing({ status: 'plain', plain: 'la la' }), intents: { media() {} }, lyricsEnabled: true }), /data-lyrics="true"/);
  const off = render(np.NowPlaying, { model: playing({ status: 'off' }), intents: { media() {} }, lyricsEnabled: false });
  assert.match(off, /data-lyrics="false"/); assert.doesNotMatch(off, /np-lyrics-line/);  // unchanged: closed until the quote button is pressed
  assert.equal(np.lyricsStageVisible({ status: 'off', plain: null }, false), true);  // and the opt-in prompt still gets the stage then
});

test('Now playing: hiding lyrics by hand still hides the note too, and the white-disc play button markup is unchanged', () => {
  const html = render(np.NowPlaying, { model: playing({ status: 'none' }), intents: { media() {} }, lyricsEnabled: true });
  assert.match(html, /class="transport-main" aria-label="Pause" data-playing="true"/);
  assert.equal(np.lyricsStageVisible({ status: 'none', plain: null }, true), false);
  assert.equal(np.lyricsStageVisible({ status: 'none', plain: null }, false), true);
});
