import { describe, expect, it } from 'vitest';
import { arrangementReducer as reduce, createArrangement } from '../src/arrangement';
import type { ArrangeAction } from '../src/arrangement';
import type { GridItem } from '../src/models';
const items: GridItem[] = [{ id: 'terminal', enabled: false, locked: true }, { id: 'hidden', enabled: false, locked: false }, { id: 'timer', enabled: true, locked: false }, { id: 'settings', enabled: true, locked: true }];
const begin = () => reduce(createArrangement(items), { type: 'begin' });
describe('launcher-arrange-core rules', () => {
  it('last visible past end with trailing hidden is a no-op', () => {
    const initial = reduce(createArrangement([...items, { id: 'trailing-hidden', enabled: false, locked: false }]), { type: 'begin' });
    const moved = reduce(initial, { type: 'move', id: 'settings', to: 999 });
    expect(moved.items).toEqual(initial.items);
    expect(reduce(moved, { type: 'commit' }).commit).toBeNull();
  });
  it('accepts exactly 64-byte module id', () => {
    const id = 'é'.repeat(32);
    expect(createArrangement([{ id, enabled: true, locked: false }]).items[0].id).toBe(id);
  });
  it('rejects exactly 65-byte module id', () => {
    expect(() => createArrangement([{ id: 'é'.repeat(32) + 'a', enabled: true, locked: false }])).toThrow('IdTooLong');
  });
  it('locked never disabled, even with disabled input', () => { const initial = begin(); expect(initial.items[0].enabled).toBe(true); const next = reduce(initial, { type: 'disable', id: 'terminal' }); expect(next.error).toBe('Locked'); expect(next.items).toEqual(initial.items); });
  it('re-enabled goes last visible, before trailing hidden', () => { const next = reduce(begin(), { type: 'enable', id: 'hidden' }); expect(next.items.filter(i => i.enabled).map(i => i.id)).toEqual(['terminal', 'timer', 'settings', 'hidden']); });
  it('cancel restores the exact snapshot', () => { const initial = begin(); const next = reduce(reduce(reduce(initial, { type: 'move', id: 'timer', to: 0 }), { type: 'disable', id: 'timer' }), { type: 'cancel' }); expect(next.items).toEqual(initial.snapshot); expect(next.snapshot).toBeNull(); });
  it('commit is None when unchanged and session ends', () => { const next = reduce(begin(), { type: 'commit' }); expect(next.commit).toBeNull(); expect(next.snapshot).toBeNull(); });
  it('commit returns full-permutation order including hidden and enabled diff', () => { const next = reduce(reduce(reduce(begin(), { type: 'move', id: 'settings', to: 0 }), { type: 'disable', id: 'timer' }), { type: 'commit' }); expect(next.commit).toEqual({ order: ['settings', 'terminal', 'hidden', 'timer'], enabledChanges: [{ id: 'timer', enabled: false }] }); expect(new Set(next.commit!.order)).toEqual(new Set(items.map(i => i.id))); });
  it('moves use visible indices and preserve hidden relative order, clamps destination', () => { const next = reduce(begin(), { type: 'move', id: 'terminal', to: 999 }); expect(next.items.map(i => i.id)).toEqual(['hidden', 'timer', 'settings', 'terminal']); });
  it('out-and-back with contiguous visible items commits None', () => { let s = reduce(createArrangement(items.filter(i => i.enabled || i.locked)), { type: 'begin' }); s = reduce(s, { type: 'move', id: 'terminal', to: 2 }); s = reduce(s, { type: 'move', id: 'terminal', to: 0 }); expect(reduce(s, { type: 'commit' }).commit).toBeNull(); });
  it('out-and-back crossing hidden items can change the full permutation', () => { let s = begin(); s = reduce(s, { type: 'move', id: 'terminal', to: 2 }); s = reduce(s, { type: 'move', id: 'terminal', to: 0 }); expect(reduce(s, { type: 'commit' }).commit?.order).toEqual(['hidden', 'terminal', 'timer', 'settings']); });
  it('reconcile updates snapshot too: cancel never resurrects removed or drops new', () => { let s = begin(); s = reduce(s, { type: 'disable', id: 'timer' }); s = reduce(s, { type: 'reconcile', items: [{ id: 'terminal', enabled: true, locked: true }, { id: 'timer', enabled: true, locked: true }, { id: 'new', enabled: true, locked: false }] }); s = reduce(s, { type: 'cancel' }); expect(s.items).toEqual([{ id: 'terminal', enabled: true, locked: true }, { id: 'timer', enabled: true, locked: true }, { id: 'new', enabled: true, locked: false }]); });
  it('rejects mutation outside an edit session and repeated begin', () => { expect(reduce(createArrangement(items), { type: 'disable', id: 'timer' }).error).toBe('NotEditing'); expect(reduce(begin(), { type: 'begin' }).error).toBe('AlreadyEditing'); });
  it('validates constructor ids and bound without normalizing', () => { expect(() => createArrangement([{ id: '', enabled: true, locked: false }])).toThrow('EmptyIds'); expect(() => createArrangement([...items, items[0]])).toThrow('DuplicateId'); expect(() => createArrangement([{ id: 'é'.repeat(33), enabled: true, locked: false }])).toThrow('IdTooLong'); expect(() => createArrangement(Array.from({ length: 257 }, (_, i) => ({ id: String(i), enabled: true, locked: false })))).toThrow('TooManyItems'); });
  it('enable visible is no-op and disabled hidden rejects move', () => { const s = begin(); expect(reduce(s, { type: 'enable', id: 'timer' }).items).toEqual(s.items); expect(reduce(s, { type: 'move', id: 'hidden', to: 0 }).error).toBe('NotVisible'); });
});
// Rust public-API traces generated by tools/generate-contracts.py, never hand-authored.
import fixture from './arrangement-fixtures.json';
describe('Rust public-API conformance', () => {
  for (const [n, trace] of fixture.entries()) it(`matches Rust trace ${n}`, () => {
    let state = createArrangement(trace.initial);
    for (const step of trace.steps) {
      state = reduce(state, step.action as ArrangeAction);
      expect({ visible: state.items.filter(i => i.enabled), hidden: state.items.filter(i => !i.enabled), editing: state.snapshot !== null, commit: state.commit, error: state.error }).toEqual(step.expected);
    }
  });
});
