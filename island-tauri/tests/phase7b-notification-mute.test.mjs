import assert from 'node:assert/strict';
import { test } from 'node:test';
import { driver, labeled } from './phase7b-runtime-harness.mjs';

const folder = 'ui/island/notifications/';
const defaults = { app: 'Updater', popup: true, sound: true, urgency: 'app', soundId: '' };
const muted = { ...defaults, popup: false, sound: false };
const custom = { ...defaults, urgency: 'critical', soundId: 'custom', popup: false };

test('mute writes one exact app rule and undo removes only the newly added rule', async () => {
  const d = driver({ 'ui/stores/index.ts': {}, 'ui/ipc/notifications.ts': {} });
  const { muteFromNotification } = d.load(folder + 'notification-mute.ts');
  const other = { ...defaults, app: 'updater' };
  const settings = { notificationAppRules: [other] };
  const save = async update => Object.assign(settings, update(settings));
  const undo = await muteFromNotification('Updater', save);
  assert.deepEqual(settings, { notificationAppRules: [other, muted] });
  const later = { ...defaults, app: 'Another app' };
  settings.notificationAppRules.push(later);
  await undo();
  assert.deepEqual(settings.notificationAppRules, [other, later]);
});

test('mute preserves custom rule fields and undo restores the previous switches', async () => {
  const d = driver({ 'ui/stores/index.ts': {}, 'ui/ipc/notifications.ts': {} });
  const { muteFromNotification } = d.load(folder + 'notification-mute.ts');
  const settings = { notificationAppRules: [custom] };
  const undo = await muteFromNotification('Updater', async update => Object.assign(settings, update(settings)));
  assert.deepEqual(settings.notificationAppRules, [{ ...custom, sound: false }]);
  await undo();
  assert.deepEqual(settings.notificationAppRules, [custom]);
});

test('Muted apps lists only history-only silent apps; Unmute drops defaults but keeps overrides', () => {
  const d = driver();
  const { MutedApps } = d.load('ui/views/settings/MutedApps.tsx');
  const stillAudible = { ...custom, app: 'Audible' };
  for (const [rule, expected] of [[muted, []], [{ ...custom, sound: false }, [{ ...custom, popup: true }]]]) {
    const rules = [rule, stillAudible];
    let patch;
    const tree = MutedApps({ rules, onSave: update => { patch = update(rules); } });
    assert.ok(labeled(tree, 'Muted apps'));
    assert.equal(labeled(tree, 'Unmute Audible'), undefined);
    labeled(tree, 'Unmute Updater').props.onClick();
    assert.deepEqual(patch, { notificationAppRules: [...expected, stillAudible] });
  }
  assert.equal(MutedApps({ rules: [stillAudible], onSave: assert.fail }).props.children, 'No muted apps');
  const { searchRows, settingLocation } = d.load('ui/views/settings-model.ts');
  assert.ok(searchRows('unmute', {}).some(g => g.rows.some(row => row.id === 'mutedApps')));
  assert.ok(settingLocation('mutedApps'));
});
