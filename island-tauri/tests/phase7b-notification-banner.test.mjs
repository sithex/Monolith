import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { component, driver, globals, labeled, nodes } from './phase7b-runtime-harness.mjs';

const note = (id, appName = 'Updater', urgency = 'normal') => ({
  notification: { id, appName, title: id, body: 'Details', urgency, actions: [], source: 'system' },
  remainingMs: urgency === 'critical' ? null : 5000, ttlMs: 5000, paused: false,
});
const file = 'ui/island/notifications/NotificationCards.tsx';

function setup(items) {
  const settings = { notificationsEnabled: true, notificationAppRules: [] };
  const state = { mode: 'large', dnd: false, gameMode: false };
  const snapshot = { items }, ops = [];
  const d = driver({
    'ui/stores/index.ts': { useSettings: () => settings, runtimeStore: { getSnapshot: () => ({}) } },
    'ui/ipc/island-state.ts': { useIslandState: () => state },
    'ui/ipc/notifications.ts': { usePopups: () => snapshot, changePopup: async op => { ops.push(op); } },
    'ui/ipc/client.ts': {}, 'framer-motion': { motion: { div: 'motion-div' }, AnimatePresence: 'presence' },
  });
  const { NotchNotifications } = d.load(file);
  const props = { fail: assert.fail, placement: 'open', mute: (...args) => ops.push(args) };
  const render = () => d.render(NotchNotifications, props);
  const renderBanner = () => d.renderOnly(() => {
    const element = component(NotchNotifications(props), 'PlainCard');
    return element.type(element.props);
  });
  return { d, render, renderBanner, settings, state, snapshot, ops };
}

test('open placement occupies its own row in the content and renders no card; newest replaces it and counts queued items', () => {
  const island = readFileSync(new URL('../ui/island/Island.tsx', import.meta.url), 'utf8');
  assert.match(island, /!active && <AnimatePresence>\{state.mode === 'hover'/);
  assert.match(island, /notice=\{active \? <NotchNotifications fail=\{fail\} placement="open"[^>]+> : undefined\}>/);
  assert.doesNotMatch(island, /className="terminal-wrap island-content"[^>]+>\s*\{active && <NotchNotifications/);
  // Under the notch the shell's title row rises 42px into the clock bar, so the banner must sit
  // after the header inside the shell, not before the shell (where the row would cover it).
  const shell = readFileSync(new URL('../ui/island/components/IslandShell.tsx', import.meta.url), 'utf8');
  assert.match(shell, /<\/header>\s*\{notice\}\s*<div className="shell-content">/);
  const { render, snapshot } = setup([note('First')]);
  assert.equal(component(render(), 'PlainCard').props.queued, 0);
  snapshot.items = [note('Newest'), ...snapshot.items];
  const tree = render();
  assert.equal(nodes(tree).filter(n => n.type?.name === 'PlainCard').length, 1);
  assert.equal(component(tree, 'NotificationCard'), undefined);
  const banner = component(tree, 'PlainCard');
  assert.equal(banner.props.popup.notification.id, 'Newest');
  assert.equal(banner.props.queued, 1);
});

test('banner follows Rust snapshots for expiry and uses normal pause, default action, dismiss and mute operations', () => {
  const regular = note('Regular'), critical = note('Critical', 'Monitor', 'critical');
  const { d, renderBanner, ops } = setup([regular, critical]);
  const banner = renderBanner();
  banner.props.onPointerEnter(); banner.props.onPointerLeave();
  nodes(banner).find(n => n.props?.className === 'notification-banner-content').props.onClick();
  labeled(banner, 'Dismiss Regular').props.onClick();
  const mute = component(banner, 'MuteButton');
  labeled(mute.type(mute.props), 'Mute Updater').props.onClick({ stopPropagation() {} });
  assert.deepEqual(ops, [
    { type: 'pause', id: 'Regular', paused: true }, { type: 'pause', id: 'Regular', paused: false },
    { type: 'invoke', id: 'Regular', key: 'default' }, { type: 'dismiss', id: 'Regular' }, ['Updater', 'Regular'],
  ]);
  d.dispose();
  const expiry = setup([regular, critical]);
  expiry.snapshot.items = [critical]; // normal item expires in Rust; critical remains the next banner
  assert.equal(component(expiry.render(), 'PlainCard').props.popup, critical);
  expiry.snapshot.items = [];
  assert.equal(expiry.render(), null);
});

test('open banner filters muted apps and DND, and reduced motion appears without entrance animation', () => {
  const { render, settings, state, snapshot } = setup([note('Quiet')]);
  settings.notificationAppRules = [{ app: 'Updater', popup: false, sound: false }];
  assert.equal(render(), null);
  snapshot.items = [note('Allowed', 'Mail'), ...snapshot.items];
  assert.equal(component(render(), 'PlainCard').props.queued, 0);
  state.dnd = true;
  assert.equal(render(), null);
  state.dnd = false;
  const initial = () => nodes(render()).find(n => n.type === 'motion-div').props.initial;
  assert.deepEqual(initial(), { opacity: 0, y: -4 });
  globals({ matchMedia: () => ({ matches: true }) }, () => assert.equal(initial(), false));
  settings.reduceMotion = true;
  assert.equal(initial(), false);
});
