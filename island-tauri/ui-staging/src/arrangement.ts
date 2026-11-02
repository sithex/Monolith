import type { ArrangeCommit, GridItem } from './models';
export interface ArrangeState { items: GridItem[]; snapshot: GridItem[] | null; commit: ArrangeCommit | null; error: string | null }
export type ArrangeAction = { type: 'begin' } | { type: 'cancel' } | { type: 'commit' } | { type: 'move'; id: string; to: number } | { type: 'disable'; id: string } | { type: 'enable'; id: string } | { type: 'reconcile'; items: GridItem[] };
const copy = (items: GridItem[]) => items.map(item => ({ ...item }));
export function validateItems(items: GridItem[]): void {
  if (items.some(i => !i.id.length)) throw new Error('EmptyIds');
  if (items.some(i => new TextEncoder().encode(i.id).length > 64)) throw new Error('IdTooLong');
  if (new Set(items.map(i => i.id)).size !== items.length) throw new Error('DuplicateId');
  if (items.length > 256) throw new Error('TooManyItems');
}
export function createArrangement(items: GridItem[]): ArrangeState {
  validateItems(items);
  return { items: items.map(i => ({ id: i.id, enabled: i.enabled || i.locked, locked: i.locked })), snapshot: null, commit: null, error: null };
}
function reconcile(base: GridItem[], incoming: GridItem[]): GridItem[] {
  const surviving = base.flatMap(item => {
    const current = incoming.find(i => i.id === item.id);
    return current ? [{ ...item, locked: current.locked, enabled: item.enabled || current.locked }] : [];
  });
  return [...surviving, ...incoming.filter(i => !base.some(b => b.id === i.id)).map(i => ({ ...i, enabled: i.enabled || i.locked }))];
}
export function arrangementReducer(state: ArrangeState, action: ArrangeAction): ArrangeState {
  const next: ArrangeState = { ...state, error: null, commit: null };
  if (action.type === 'reconcile') {
    // Adapter passes a validated module list, just like the Rust core's caller.
    validateItems(action.items);
    return { ...next, items: reconcile(state.items, action.items), snapshot: state.snapshot && reconcile(state.snapshot, action.items) };
  }
  if (action.type === 'begin') return state.snapshot ? { ...next, error: 'AlreadyEditing' } : { ...next, snapshot: copy(state.items) };
  if (!state.snapshot) return { ...next, error: 'NotEditing' };
  if (action.type === 'cancel') return { ...next, items: copy(state.snapshot), snapshot: null };
  if (action.type === 'commit') {
    const changed = JSON.stringify(state.items) !== JSON.stringify(state.snapshot);
    return { ...next, snapshot: null, commit: changed ? {
      order: state.items.map(i => i.id),
      enabledChanges: state.items.filter(i => state.snapshot!.find(b => b.id === i.id)?.enabled !== i.enabled).map(i => ({ id: i.id, enabled: i.enabled })),
    } : null };
  }
  const index = state.items.findIndex(i => i.id === action.id);
  if (index < 0) return { ...next, error: 'UnknownId' };
  const item = state.items[index];
  if (action.type === 'disable') {
    if (item.locked) return { ...next, error: 'Locked' };
    return { ...next, items: state.items.map(i => i.id === action.id ? { ...i, enabled: false } : i) };
  }
  if (action.type === 'enable') {
    if (item.enabled) return next;
    const items = copy(state.items);
    items.splice(index, 1);
    const last = items.reduce((found, i, n) => i.enabled ? n : found, -1);
    items.splice(last + 1, 0, { ...item, enabled: true });
    return { ...next, items };
  }
  if (!item.enabled) return { ...next, error: 'NotVisible' };
  const visible = state.items.filter(i => i.enabled);
  const from = visible.findIndex(i => i.id === action.id);
  const to = Math.max(0, Math.min(Math.trunc(action.to), visible.length - 1));
  if (from === to) return next;
  // Remove/insert the actual item, not a rewrite of visible slots. Hidden
  // items retain relative order, but can shift indices as neighbours move.
  const items = copy(state.items);
  items.splice(index, 1);
  const target = items.map((i, n) => i.enabled ? n : -1).filter(n => n >= 0)[to] ?? items.length;
  items.splice(target, 0, item);
  return { ...next, items };
}
