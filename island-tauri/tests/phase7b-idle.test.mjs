import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const ui = name => readFileSync(new URL(`../../island/plasma-anchor/contents/ui/${name}`, import.meta.url), 'utf8');

test('online/offline is event-driven: a D-Bus name watch, no 15 s status poll', () => {
  const main = ui('main.qml');
  assert.match(main, /DBus\.DBusServiceWatcher\s*\{[^}]*watchedService: "local\.hermes\.Monolith"/s);
  assert.doesNotMatch(main, /interval: 15000/, 'the per-notch 15 s status poll is gone');
  const net = main.match(/id: statusSafetyNet[^\n]*/)?.[0] ?? '';
  const interval = Number(net.match(/interval: (\d+)/)?.[1]);
  assert.ok(interval >= 60000, `safety net must be slow, got ${interval}`);
  // Every status request is either an event follow-up or the safety net.
  const uses = [...main.matchAll(/Timer \{ id: (\w+)[^\n]*enqueue\("status"\)/g)].map(m => m[1]);
  assert.deepEqual(uses.sort(), ['islandGone', 'islandUp', 'islandUpLate', 'statusSafetyNet']);
});

test('game-mode freeze reaches every animated part of the notch', () => {
  const main = ui('main.qml');
  assert.match(main, /companionFrozen: root\.frozen/);
  assert.match(main, /enabled: root\.companion && !root\.frozen/, 'the pointer relay is not even listened to while frozen');
  const pill = ui('PanelPill.qml');
  assert.match(pill, /running: \([^\n]*\) && pill\.visible && pill\.motion && pill\.animateHere && !pill\.companionFrozen/);
  assert.match(pill, /frozen: pill\.companionFrozen/);
  const face = ui('Companion.qml');
  for (const timer of ['companionDrowsy', 'companionBlinker']) {
    const line = face.split('\n').slice(face.split('\n').findIndex(l => l.includes(timer))).slice(0, 8).join('\n');
    assert.match(line, /!face\.frozen/, `${timer} stops while frozen`);
  }
  assert.match(face, /Behavior on shownX \{ enabled: [^;]*!face\.frozen/);
});

test('the companion has no private timer left that could tick through a freeze', () => {
  // Every Timer inside the characters must mention the freeze or be absent.
  for (const file of ['Isaac.qml', 'HabitCharacter.qml']) {
    const text = ui(`characters/${file}`);
    const timers = [...text.matchAll(/Timer\s*\{[^}]*running:[^\n]*/g)];
    assert.ok(timers.length > 0 || file === 'HabitCharacter.qml', `${file}: expected timers to check`);
    for (const m of timers) assert.match(m[0], /frozen|clockWanted/, `${file}: ${m[0].slice(0, 80)}`);
  }
});
