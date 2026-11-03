import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('native notch hover is visual only and never asks the actor for a preview', () => {
  const main = readFileSync(new URL('../../island/plasma-anchor/contents/ui/main.qml', import.meta.url), 'utf8');
  assert.doesNotMatch(main, /requests\.enqueue\(["'](?:enter|leave)["']\)/,
    'panel hover must stay local: only a deliberate click opens the island');
  assert.match(main, /onActivated:\s*root\.requests\.enqueue\(\(root\.online\s*\?\s*"pin"\s*:\s*"launch"\)\s*\+\s*" "\s*\+\s*Plasmoid\.id\)/);
  assert.doesNotMatch(main, /Hover to preview/);
  assert.match(main, /toolTipMainText:\s*""/, 'no title tooltip');
  assert.match(main, /toolTipSubText:\s*""/, 'no replacement instruction popup');
});
