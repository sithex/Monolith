// Notification sound choice: picker model and wiring (global + per-app).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const sounds = load('ui/views/settings/sounds.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('sound options: default row first, installed favourites next, rest A–Z, test tones hidden', () => {
  const opts = sounds.soundOptions(['zzz-end', 'bell', 'audio-channel-front-left', 'message-new-instant', 'alpha', 'theme-demo'], 'Default');
  assert.deepEqual(opts.map(o => o.value), ['', 'message-new-instant', 'bell', 'alpha', 'zzz-end']);
  assert.equal(opts[0].label, 'Default');
  assert.equal(opts[1].label, 'Message new instant');
  assert.deepEqual(sounds.soundOptions([], 'Default').map(o => o.value), [''], 'no sounds installed: only the default');
});

test('sound choice is wired: global row, per-app rule picker, preview on pick', () => {
  assert.match(src('ui/views/SettingsView.tsx'), /<SoundPicker[^>]*value=\{settings\.notificationSoundId/);
  assert.match(src('ui/views/settings/AppRulesEditor.tsx'), /<SoundPicker[^>]*soundId/);
  assert.match(src('ui/views/settings/SoundPicker.tsx'), /notificationSoundPreview/);
  assert.match(src('ui/views/settings-model.ts'), /keys: \['notificationSoundId'\]/);
});

test('card close is a centred vector X, not a text glyph', () => {
  const card = src('ui/island/notifications/NotificationCards.tsx');
  assert.match(card, /className="notification-close"[^]*?<svg viewBox="0 0 12 12"/);
  assert.doesNotMatch(card, />×<\/button>/);
  assert.match(src('ui/island/notifications/notification-cards.css'), /\.notification-card \.notification-icon \{[^}]*margin: 0/);
});
