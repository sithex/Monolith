import { test } from 'node:test';
import assert from 'node:assert/strict';
import { driver, nodes, globals, component } from './phase7b-runtime-harness.mjs';

const fail = error => { throw error; };
const eventWindow = () => {
  const listeners = new Map();
  return {
    listeners, setTimeout() {}, clearTimeout() {},
    addEventListener(type, fn) { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(fn); },
    removeEventListener(type, fn) { listeners.get(type)?.delete(fn); },
    emit(type, event) { for (const fn of listeners.get(type) ?? []) fn(event); },
  };
};

// Separate drivers retain each component's hook ownership. State writes schedule only
// that component; the test executes the actual lyrics stage whenever its parent changes.
test('playhead ticks advance progress without rendering the lyrics stage', () => {
  let monotonic = 0, stageRenders = 0, timerId = 0;
  const intervals = new Map();
  const win = eventWindow(), doc = { visibilityState: 'visible', addEventListener() {}, removeEventListener() {} };
  globals({ window: win, performance: { now: () => monotonic }, setInterval: (fn, ms) => { assert.equal(ms, 250); intervals.set(++timerId, fn); return timerId; }, clearInterval: id => intervals.delete(id) }, () => {
    const parent = driver({ 'ui/island/components/motion-preference.ts': { useReducedMotionAll: () => false } });
    const progress = driver(), stage = driver({ 'framer-motion': { motion: { div: 'div' } } });
    const { NowPlaying } = parent.load('ui/island/components/NowPlaying.tsx');
    const { TrackProgress } = progress.load('ui/island/components/NowPlaying.tsx');
    const { LyricsStage } = stage.load('ui/island/components/NowPlaying.tsx');
    const sampledAtMs = Date.now();
    const track = { player: 'fixture', title: 'Song', playing: true, status: 'playing', positionMs: 10000, durationMs: 180000, sampledAtMs, rate: 1, canControl: true, canSeek: true };
    const model = { nowPlaying: track, lyrics: { status: 'synced', activeIndex: 0, lines: [{ timeMs: 0, text: 'First' }, { timeMs: 12000, text: 'Next' }] } };
    const props = { model, intents: { media() {} } };
    let tree = parent.renderOnly(NowPlaying, props);
    tree.props.ref.current = {};
    globals({ document: doc }, () => parent.flushEffects());
    const renderStage = () => { stageRenders++; return stage.render(LyricsStage, component(tree, 'LyricsStage').props); };
    renderStage();
    const progressProps = component(tree, 'TrackProgress').props;
    try {
      globals({ document: doc }, () => {
        // Re-rendering the parent after the ref attaches would start a misplaced clock.
        tree = parent.render(NowPlaying, props);
        let playhead = progress.render(TrackProgress, progressProps);
        const before = parent.updates();
        assert.equal(intervals.size, 1);
        for (let i = 1; i <= 8; i++) {
          monotonic = i * 250; for (const tick of [...intervals.values()]) tick();
          if (parent.updates() !== before) { tree = parent.render(NowPlaying, props); renderStage(); }
          playhead = progress.render(TrackProgress, progressProps);
        }
        assert.equal(parent.updates(), before, 'ticks must schedule the playhead, never its parent');
        assert.equal(stageRenders, 1, 'no lyric reconciliation on the eight ticks');
        assert.ok(nodes(playhead).find(n => n.type === 'progress').props.value >= 12000);
        model.lyrics = { ...model.lyrics, activeIndex: 1 };
        tree = parent.render(NowPlaying, props);
        const lyrics = renderStage();
        assert.equal(nodes(lyrics).filter(n => n.type === 'li')[1].props['aria-current'], 'true', 'host lyric updates still reach the stage');
      });
    } finally { globals({ document: doc }, () => { parent.dispose(); progress.dispose(); stage.dispose(); }); }
  });
});

const empty = () => null;
function islandHarness() {
  let state = { mode: 'large', view: 'terminal', terminalStarted: false }, settings = { animationSpeed: 100 };
  const calls = [], speedCalls = [], motion = driver().load('ui/island/motion-profile.ts');
  const h = driver({
    'framer-motion': { motion: { div: 'div' }, AnimatePresence: 'presence', MotionConfig: 'motion-config' },
    'ui/ipc/island-state.ts': { useIslandState: () => state, dispatch: async intent => { calls.push(intent); }, subscribeCue: () => () => {} },
    'ui/ipc/client.ts': { notchSchemeSet: async () => {} },
    'ui/stores/index.ts': { useSettings: () => settings, usePalette: () => ({}), useAppearance: () => 'neutral', runtimeStore: { getSnapshot: () => ({}) } },
    'ui/stores/apps.ts': { useLauncher: () => ({ items: [] }), useMedia: () => null, configureAppTopics: async () => {} },
    'ui/island/IslandSurface.tsx': { IslandSurface: 'surface' },
    'ui/island/components/IslandShell.tsx': { IslandShell: 'shell' },
    'ui/island/Topline.tsx': { Topline: empty }, 'ui/island/ViewPager.tsx': { ViewPager: 'pager' },
    'ui/island/Launcher.tsx': { Launcher: empty, LauncherEditButton: empty, viewLabels: { terminal: 'Terminal', launcher: 'Apps', monitor: 'Monitor', settings: 'Settings' } },
    'ui/island/SideControls.tsx': { SideControls: 'sides' },
    'ui/island/CommandPalette.tsx': { CommandPalette: empty }, 'ui/island/VoiceControls.tsx': { VoiceControls: empty },
    'ui/island/components/ColorField.tsx': { ColorPickerPanel: empty },
    ...Object.fromEntries(['AppsView', 'SettingsView', 'MonitorView', 'AiStatsView'].map(name => [`ui/views/${name}.tsx`, { [name]: empty }])),
    'ui/widgets/Toast.tsx': { Toast: empty },
    'ui/island/PreviewNotice.tsx': { PreviewNotice: empty },
    'ui/island/notifications/NotificationCards.tsx': { NotchNotifications: empty, NotificationCards: empty, useNotificationHover: () => ({ active: false }) },
    'ui/island/ApprovalNotice.tsx': { ApprovalNotice: empty, useApprovalHover: () => ({ active: false }) },
    'ui/island/click-sound.ts': { createClickSound: () => () => {} },
    'ui/island/tokens/schemes.ts': { schemePalette: () => null, schemeById: () => ({}), isLightPalette: () => false },
    'ui/island/hermes-theme.ts': { useHermesThemeSync() {}, themePayload: () => ({}) },
    'ui/island/album-scheme.ts': { useAlbumScheme: () => null },
    'ui/island/components/motion-preference.ts': { useReducedMotionAll: () => false },
    'ui/island/motion-profile.ts': { ...motion, setMotionSpeed: percent => { speedCalls.push(percent); motion.setMotionSpeed(percent); } },
  });
  const { Island } = h.load('ui/island/Island.tsx');
  return { ...h, calls, speedCalls, motion, setView(view) { state = { ...state, view }; }, setMode(mode) { state = { ...state, mode }; },
    setSpeed(animationSpeed) { settings = { ...settings, animationSpeed }; }, renderIsland() { h.render(Island); return h.render(Island); }, renderOnlyIsland: () => h.renderOnly(Island) };
}
function withIsland(run) {
  globals({ window: eventWindow(), document: { documentElement: { dataset: {}, style: { setProperty() {} } } } }, () => {
    const h = islandHarness(); try { run(h); } finally { h.dispose(); }
  });
}
const back = tree => nodes(tree).find(n => n.type === 'shell').props.back;
test('Back history records only committed views and preserves forward and Back navigation', () => {
  withIsland(h => {
    let tree = h.renderIsland();
    h.setView('launcher'); tree = h.renderIsland();
    h.setView('monitor'); tree = h.renderIsland();
    assert.equal(back(tree).label, 'Apps');
    // This speculative render must not push Monitor into the committed Back stack.
    h.setView('settings'); const speculative = h.renderOnlyIsland();
    assert.equal(back(speculative).label, 'Apps', 'render must not push Monitor into the committed stack');
    back(tree).go();
    assert.deepEqual(h.calls.at(-1), { type: 'selectView', view: 'launcher' });
  });
  withIsland(h => {
    h.renderIsland(); h.setView('launcher'); h.renderIsland(); h.setView('monitor');
    let tree = h.renderIsland(); back(tree).go(); h.setView('launcher'); tree = h.renderIsland();
    assert.equal(back(tree).label, 'Terminal');
    assert.equal(nodes(tree).find(n => n.type === 'sides').props.back.view, 'terminal');
    back(tree).go(); h.setView('terminal'); tree = h.renderIsland();
    back(tree).go(); assert.equal(h.calls.at(-1).view, 'terminal');
    h.setView('monitor'); tree = h.renderIsland(); assert.equal(back(tree).label, 'Terminal');
  });
});
test('motion speed publishes after commit and rebuilds transitions at the committed speed', () => {
  withIsland(h => {
    h.renderIsland(); const count = h.speedCalls.length;
    h.setSpeed(200); h.renderOnlyIsland();
    assert.equal(h.speedCalls.length, count, 'render cannot publish a speculative setting');
    assert.equal(h.motion.motionSpeed(), 1);
    h.flushEffects();
    assert.equal(h.motion.motionSpeed(), 2);
    const tree = h.renderIsland();
    const wrap = nodes(tree).find(n => n.props?.className === 'terminal-wrap island-content');
    assert.equal(wrap.props.transition.duration, .15);
    h.renderIsland(); assert.equal(h.speedCalls.length, count + 1, 'unrelated renders do not publish speed');
  });
});
test('closing content is inert through the fade and becomes interactive on reopen', () => {
  withIsland(h => {
    const wrap = tree => nodes(tree).find(n => n.props?.className === 'terminal-wrap island-content');
    const open = wrap(h.renderIsland());
    assert.equal(open.props.inert, false);
    open.props.ref.current = { offsetLeft: 0, offsetTop: 42, offsetWidth: 900, offsetHeight: 600 };
    h.setMode('still'); let closing = wrap(h.renderIsland());
    assert.equal(closing.props.hidden, false, 'the outgoing content lingers');
    assert.equal(closing.props.inert, true); assert.equal(closing.props['aria-hidden'], true);
    h.setMode('large'); assert.equal(wrap(h.renderIsland()).props.inert, false);
  });
});

test('Escape outside the island collapses only while large and respects input and handled events', () => {
  const win = eventWindow(), calls = [];
  let state = { rev: 1, mode: 'hover', view: 'terminal' }, focused = 0;
  class Element { constructor(protectedInput = false) { this.protectedInput = protectedInput; } closest() { return this.protectedInput ? this : null; } focus() { focused++; } }
  globals({ window: win, Element, innerWidth: 1600, innerHeight: 1000, addEventListener() {}, removeEventListener() {} }, () => {
    const h = driver({
      'framer-motion': { motion: { section: 'section' }, useAnimationControls: () => ({ stop() {}, start: async () => {} }) },
      'ui/island/components/motion-preference.ts': { useReducedMotionAll: () => true },
      'ui/ipc/island-state.ts': { useIslandState: () => state, dispatch: async intent => calls.push(intent) },
      'ui/ipc/client.ts': {},
      'ui/stores/index.ts': { useSettings: () => ({}), usePalette: () => ({}) },
      'ui/stores/apps.ts': { useLayout: () => ({ rev: 1, unchangedSizing: false, layout: { preset: { type: 'custom', width: 900, maxHeight: 900 }, buttons: [], gap: 10, buttonDiameter: 36 } }), useLauncher: () => ({ items: [] }) },
      'ui/island/components/IslandShell.tsx': { UI_SPRING: {} },
      'ui/island/ApprovalNotice.tsx': { useApprovalHover: () => ({ active: false, width: 360, height: 116 }) },
      'ui/island/notifications/notification-hover.ts': { useNotificationHover: () => ({ active: false, width: 420, height: 200 }) },
    });
    const { IslandSurface } = h.load('ui/island/IslandSurface.tsx');
    const render = () => h.render(IslandSurface, { children: null, fail });
    const escape = overrides => win.emit('keydown', { key: 'Escape', defaultPrevented: false, target: new Element(), ...overrides });
    try {
      render(); escape(); assert.equal(calls.length, 0);
      state = { ...state, mode: 'large' }; render();
      escape(); assert.deepEqual(calls, [{ type: 'collapse' }], 'body or any target outside the island reaches the window listener');
      escape({ defaultPrevented: true }); escape({ target: new Element(true) }); escape({ key: 'Enter' });
      assert.equal(calls.length, 1, 'terminal/forms and controls handling Escape retain it');
      state = { ...state, mode: 'still' }; render(); escape(); assert.equal(calls.length, 1);
      assert.equal(win.listeners.get('keydown').size, 0);
      state = { ...state, mode: 'large' }; render(); escape(); assert.equal(calls.length, 2);
      assert.equal(focused, 0, 'neither hover nor large asks a DOM control for focus');
    } finally { h.dispose(); }
    assert.equal(win.listeners.get('keydown').size, 0, 'unmount removes the listener');
  });
});
