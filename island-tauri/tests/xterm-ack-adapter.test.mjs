import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createParseAck, ACK_INTERVAL_MS } from '../ui/ipc/xterm-ack-core.ts';
import { toCanonicalMode, toLegacyMode } from '../ui/ipc/legacy-mode.ts';

function fixture() {
  const acks = [], scheduled = [];
  class FakeTerminal {
    writes = [];
    write(data, callback) { this.writes.push({ data, callback }); }
    parse(index) { this.writes[index].callback?.(); }
  }
  const original = FakeTerminal.prototype.write;
  const adapter = createParseAck(async (seq) => { acks.push(seq); }, (fn) => scheduled.push(fn));
  adapter.install(FakeTerminal.prototype);
  const terminal = new FakeTerminal();
  return { acks, adapter, terminal, original, flush() { while (scheduled.length) scheduled.shift()(); } };
}

test('no credit before the xterm parse callback (not delivery or write)', () => {
  const f = fixture();
  f.adapter.record({seq: 1, data: 'alpha'});
  f.flush(); assert.deepEqual(f.acks, []);
  f.terminal.write('alpha'); f.flush(); assert.deepEqual(f.acks, []);
  f.terminal.parse(0); f.flush(); assert.deepEqual(f.acks, [1]);
});
test('cumulative parse acknowledgements batch within 16 ms and preserve original callback', () => {
  assert.equal(ACK_INTERVAL_MS, 16);
  const f = fixture(); let called = 0;
  for (const seq of [1, 2, 3]) {
    f.adapter.record({seq, data: String(seq)});
    f.terminal.write(String(seq), () => called++);
    f.terminal.parse(seq - 1);
  }
  assert.equal(called, 3); assert.deepEqual(f.acks, []);
  f.flush(); assert.deepEqual(f.acks, [3]);
});
test('snapshot subsumes skipped queued packets only after replay parsing', () => {
  const f = fixture();
  f.adapter.record({seq: 1, data: 'a'}); f.adapter.record({seq: 2, data: 'b'});
  f.adapter.recordSnapshot({seq: 2, data: 'ab'});
  f.adapter.recordSnapshot({seq: 2, data: 'ab'}); // return/channel duplicate
  f.adapter.record({seq: 3, data: 'c'});
  f.terminal.write('ab'); f.flush(); assert.deepEqual(f.acks, []);
  f.terminal.parse(0); f.flush(); assert.deepEqual(f.acks, [2]);
  f.terminal.write('c'); f.terminal.parse(1); f.flush(); assert.deepEqual(f.acks, [2, 3]);
});
test('untagged writes pass through unchanged and cannot grant credit', () => {
  const f = fixture(); const cb = () => {};
  f.adapter.record({seq: 1, data: 'tagged'});
  f.terminal.write('exit banner', cb);
  assert.equal(f.terminal.writes[0].callback, cb);
  f.terminal.parse(0); f.flush(); assert.deepEqual(f.acks, []);
});
test('empty snapshot needs no parsing and credits skipped packets', () => {
  const f = fixture(); f.adapter.record({seq: 1, data: 'skip'});
  f.adapter.recordSnapshot({seq: 1, data: ''}); f.flush(); assert.deepEqual(f.acks, [1]);
  f.adapter.record({seq: 2, data: 'next'}); f.terminal.write('next');
  f.terminal.parse(0); f.flush(); assert.deepEqual(f.acks, [1, 2]);
});
test('sequence wrap, paused credit and original callback errors preserve parse completion', () => {
  const f = fixture(); f.adapter.setAckPaused(true);
  for (const [i, seq] of [0xffffffff, 0].entries()) {
    f.adapter.record({seq, data: String(seq)});
    f.terminal.write(String(seq), () => { throw Error('callback'); });
    assert.throws(() => f.terminal.parse(i), /callback/);
  }
  f.flush(); assert.deepEqual(f.acks, []);
  f.adapter.setAckPaused(false); f.flush(); assert.deepEqual(f.acks, [0]);
});
test('production adapter and legacy renderer import the identical xterm specifier', () => {
  for (const path of ['../ui/ipc/xterm-ack-adapter.ts', '../ui/legacy/renderer.jsx']) {
    assert.match(readFileSync(new URL(path, import.meta.url), 'utf8'), /from "@xterm\/xterm\/lib\/xterm\.js"/);
  }
});
test('reattach and restarted sequence zero ignore callbacks from the disposed terminal', () => {
  const f = fixture();
  f.adapter.record({seq: 900, data: 'old'}); f.terminal.write('old');
  f.adapter.reset();
  f.adapter.recordSnapshot({seq: 0, data: 'new'}); f.terminal.write('new');
  f.terminal.parse(0); f.flush(); assert.deepEqual(f.acks, []);
  f.terminal.parse(1); f.flush(); assert.deepEqual(f.acks, [0]);
});
test('shim translates every mode and rejects inherited/unknown names', () => {
  for (const [legacy, canonical] of [['idle','still'],['preview','hover'],['pinned','large']]) {
    assert.equal(toCanonicalMode(legacy), canonical); assert.equal(toLegacyMode(canonical), legacy);
  }
  for (const invalid of ['still','large','__proto__','constructor',null,{}]) assert.throws(() => toCanonicalMode(invalid));
});
