// Approval notice: pure presentation rules plus guards on the component source.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { load } from './phase7b-loader.mjs';
const { ARM_DELAY_MS, canPress, needsExpand, noticeFor, noticeHeight, pointerOnly, PLAIN_HOVER_HEIGHT, HOVER_WIDTH } = load('ui/island/approval-model.ts');
const src = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

const choice = (id, style) => ({ id, label: id, style });
const prompt = (extra = {}) => ({ id: 'ap1-1', kind: 'approval', text: 'rm -rf build', textTruncated: false, description: 'recursive delete',
  choices: [choice('once', 'once'), choice('session', 'session'), choice('always', 'always'), choice('deny', 'deny')],
  answerable: true, answered: false, note: null, more: 0, ...extra });

test('notice view: prompt wins, a bare waiting title shows the read-only row, nothing otherwise', () => {
  assert.equal(noticeFor(null), null);
  assert.equal(noticeFor({ rev: 1, prompt: null, waitingUnknown: false }), null);
  assert.deepEqual(noticeFor({ rev: 1, prompt: null, waitingUnknown: true }), { kind: 'waiting' });
  const p = prompt();
  assert.equal(noticeFor({ rev: 2, prompt: p, waitingUnknown: true }).prompt, p);
});

test('approving choices need a real pointer click; keyboard activation (detail 0) never approves', () => {
  const now = 10_000, base = { armedAt: 0, now, prompt: prompt(), pending: false };
  for (const style of ['once', 'session', 'always', 'option']) {
    assert.equal(pointerOnly(style), true, style);
    assert.equal(canPress({ ...base, style, detail: 0 }), false, `${style} by keyboard`);
    assert.equal(canPress({ ...base, style, detail: 1 }), true, `${style} by pointer`);
  }
  assert.equal(canPress({ ...base, style: 'deny', detail: 0 }), true, 'declining is always safe');
});

test('clicks before the arm delay, while pending, answered or read-only are ignored', () => {
  const base = { style: 'once', detail: 1, armedAt: 1000 + ARM_DELAY_MS, prompt: prompt(), pending: false };
  assert.equal(canPress({ ...base, now: 1000 }), false, 'too early after the prompt appeared');
  assert.equal(canPress({ ...base, now: base.armedAt }), true);
  assert.equal(canPress({ ...base, now: 9999, pending: true }), false);
  assert.equal(canPress({ ...base, now: 9999, prompt: prompt({ answered: true }) }), false);
  assert.equal(canPress({ ...base, now: 9999, prompt: prompt({ answerable: false, note: 'x' }) }), false);
  assert.ok(ARM_DELAY_MS >= 300, 'long enough for a click aimed at something else to land');
});

test('truncation: short commands need no expander, long or multi-line ones do', () => {
  assert.equal(needsExpand('ls -la'), false);
  assert.equal(needsExpand('a\nb\nc'), false);
  assert.equal(needsExpand('a\nb\nc\nd'), true);
  assert.equal(needsExpand('x'.repeat(151)), true);
});

test('hover height follows the content and grows when expanded', () => {
  assert.equal(noticeHeight(null, false), PLAIN_HOVER_HEIGHT);
  const short = noticeHeight({ kind: 'prompt', prompt: prompt() }, false);
  const long = prompt({ text: ('x'.repeat(60) + '\n').repeat(8) });
  const collapsed = noticeHeight({ kind: 'prompt', prompt: long }, false);
  const expanded = noticeHeight({ kind: 'prompt', prompt: long }, true);
  assert.ok(expanded > collapsed && collapsed >= short, `${short} ${collapsed} ${expanded}`);
  const clarify = prompt({ kind: 'clarify', choices: ['a', 'b', 'c', 'd'].map(id => choice(id, 'option')) });
  assert.ok(noticeHeight({ kind: 'prompt', prompt: clarify }, false) > short, 'a stacked list is taller than one button row');
  const readOnly = prompt({ answerable: false, note: 'Open the terminal to answer this', choices: [] });
  assert.ok(noticeHeight({ kind: 'prompt', prompt: readOnly }, false) < noticeHeight({ kind: 'prompt', prompt: clarify }, false));
  assert.ok(HOVER_WIDTH > 360);
});

test('component never answers from the keyboard and wires Esc to the existing dismiss', () => {
  const tsx = src('ui/island/ApprovalNotice.tsx');
  assert.ok(!/key\s*===\s*['"]Enter['"]/.test(tsx) && !/onKeyDown/.test(tsx), 'no Enter / key handler on buttons');
  assert.match(tsx, /e\.key === 'Escape'[\s\S]*dismissPreview/);
  assert.match(tsx, /canPress\(/, 'every answer goes through the click guard');
  assert.ok(!/autoFocus/.test(tsx), 'nothing is focused automatically');
  assert.match(tsx, /answerApproval\(p\.id, choiceId\)/, 'the answer carries the prompt id the user saw');
});

test('Island renders the notice in the hover slot and the surface takes its size from it', () => {
  const island = src('ui/island/Island.tsx'), surface = src('ui/island/IslandSurface.tsx');
  assert.match(island, /approvalHover\.active \? <ApprovalNotice fail=\{fail\} \/> : <PreviewNotice fail=\{fail\} \/>/);
  assert.match(surface, /state\.mode === 'hover' \? hover\.width/);
  assert.match(surface, /state\.mode === 'hover' \? hover\.height/);
});

test('bindings carry the prompt id in the answer and the setting exists', () => {
  const b = src('ui/ipc/bindings.ts');
  assert.match(b, /ApprovalAnswer = \{[^}]*promptId: string[^}]*choice: string/s);
  assert.match(b, /approvalsInNotch: boolean/);
  assert.match(b, /__TAURI_INVOKE\("approval_answer"/);
});
