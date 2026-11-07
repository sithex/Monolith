import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { groupByApp, sortGroups, matches, percent } = load('ui/views/process-table.ts');
const row = (pid, ppid, name, cpu, mem, extra = {}) => ({ pid, ppid, startTicks: pid * 10, name, command: `/usr/bin/${name} --x`, cpu, memoryBytes: mem, gpu: null, vramBytes: null, threads: 1, own: true, ...extra });
const rows = [row(10, 1, 'firefox', 5, 500), row(11, 10, 'firefox', 20, 300, { gpu: 12, vramBytes: 100 }), row(12, 10, 'bash', 1, 10), row(13, 12, 'vim', null, 20), row(20, 1, 'steam', 30, 50)];
test('helpers that share the parent name merge into one app row; other children stay separate', () => {
  const groups = groupByApp(rows), fx = groups.find(g => g.lead.pid === 10);
  assert.equal(fx.members.length, 2); assert.equal(fx.cpu, 25); assert.equal(fx.memory, 800); assert.equal(fx.gpu, 12); assert.equal(fx.vram, 100);
  assert.ok(groups.find(g => g.lead.pid === 12)); assert.equal(groups.find(g => g.lead.pid === 13).cpu, null, 'unknown CPU stays unknown');
});
test('sorting puts the heaviest first and unknowns last; name sorts alphabetically', () => {
  const g = groupByApp(rows);
  assert.deepEqual(sortGroups(g, 'cpu').map(x => x.lead.pid), [20, 10, 12, 13]);
  assert.deepEqual(sortGroups(g, 'memory').map(x => x.lead.pid), [10, 20, 13, 12]);
  assert.deepEqual(sortGroups(g, 'name').map(x => x.lead.name), ['bash', 'firefox', 'steam', 'vim']);
});
test('filter matches name, command or exact pid; percent formatting', () => {
  assert.ok(matches(rows[0], 'FIRE')); assert.ok(matches(rows[0], '--x')); assert.ok(matches(rows[0], '10')); assert.ok(!matches(rows[0], '1'));
  assert.equal(percent(null), '–'); assert.equal(percent(0.01), '0%'); assert.equal(percent(3.14), '3.1%'); assert.equal(percent(42.6), '43%');
});
test('Monitor has a Processes tab wired to the guarded kill command', () => {
  const view = readFileSync(new URL('../ui/views/MonitorView.tsx', import.meta.url), 'utf8');
  const panel = readFileSync(new URL('../ui/views/ProcessesPanel.tsx', import.meta.url), 'utf8');
  assert.match(view, /<ProcessesPanel active=\{active\} \/>/);
  assert.match(panel, /processKill\(t\.pid, t\.startTicks, signal\)/);
  assert.match(panel, /filter\(m => m\.own\)/, 'only the user\'s own processes are signalled');
  assert.match(panel, /setConfirm\(\{ key: g\.key, signal: 'kill' \}\)/, 'kill needs a confirm step');
});
