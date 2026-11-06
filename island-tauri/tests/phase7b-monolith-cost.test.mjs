import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { formatCpu, formatMemory, formatWakeups, costRows, windowNote } from '../ui/views/monolith-cost.ts';

const reading = (extra = {}) => ({
  cpu: 3.14159, pssBytes: 612 * 1024 * 1024, wakeups: 87.34, processes: 6, threads: 91,
  plasmashellCpu: 2.3, kwinCpu: 12.6, windowSecs: 10, ...extra,
});

test('unavailable values read as a dash, never as zero', () => {
  assert.equal(formatCpu(null), '—');
  assert.equal(formatCpu(undefined), '—');
  assert.equal(formatCpu(Number.NaN), '—');
  assert.equal(formatMemory(null), '—');
  assert.equal(formatWakeups(null), '—');
  assert.equal(formatCpu(0), '0.0%', 'a real zero is a number');
});

test('formats keep the card compact', () => {
  assert.equal(formatCpu(3.14159), '3.1%');
  assert.equal(formatCpu(12.6), '13%');
  assert.equal(formatMemory(612 * 1024 * 1024), '612 MB');
  assert.equal(formatMemory(1.5 * 1024 * 1024 * 1024), '1.5 GB');
  assert.equal(formatWakeups(87.34), '87.3/s');
  assert.equal(formatWakeups(1234.5), '1235/s');
});

test('rows cover the island tree, plasmashell and kwin in order', () => {
  const rows = costRows(reading());
  assert.deepEqual(rows.map(r => r.label), [
    'Monolith CPU', 'Monolith memory (PSS)', 'Monolith wakeups', 'plasmashell CPU', 'kwin_wayland CPU',
  ]);
  assert.deepEqual(rows.map(r => r.value), ['3.1%', '612 MB', '87.3/s', '2.3%', '13%']);
  assert.match(rows[0].hint, /6 processes · 91 threads/);
  assert.deepEqual(costRows(null).map(r => r.value), ['—', '—', '—', '—', '—']);
  assert.equal(costRows(reading({ kwinCpu: null }))[4].value, '—', 'kwin not running');
});

test('the footnote reports the window actually covered', () => {
  assert.match(windowNote(null), /first reading takes a moment/);
  assert.match(windowNote(reading({ windowSecs: 0 })), /first reading takes a moment/);
  assert.match(windowNote(reading({ windowSecs: 4.2 })), /last 4 s/);
});

test('the card samples only while the Monitor page is showing', () => {
  const card = readFileSync(new URL('../ui/views/MonolithCostCard.tsx', import.meta.url), 'utf8');
  assert.match(card, /poller\.setVisible\(active\)/);
  assert.match(card, /poller\.dispose\(\)/);
  const monitor = readFileSync(new URL('../ui/views/MonitorView.tsx', import.meta.url), 'utf8');
  assert.match(monitor, /<MonolithCostCard active=\{active\} \/>/);
});
