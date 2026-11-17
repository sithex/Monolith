import type { NotificationItem } from './app-presentations';

/** One row of the history list: a notification, or N identical repeats folded into it. */
export type NotificationRow = {
  key: string;
  /** Every stored notification this row stands for, newest first. Dismissing the row dismisses each one. */
  ids: string[];
  count: number;
  /** The newest member; its title, body, icon and age label are what the row shows. */
  item: NotificationItem;
};
export type NotificationGroup = { name: string; count: number; rows: NotificationRow[] };

/** Repeats only fold when nothing about them needs its own row: no actions, nothing critical, no progress. */
function foldable(item: NotificationItem): boolean {
  return item.actions.length === 0 && item.urgency !== 'critical' && item.progress === null;
}

/**
 * Group the history by app, newest first, and fold identical repeats (same title, body and urgency) inside
 * a group into one counted row. A pure view over the stored list: nothing is dropped or rewritten, and every
 * stored id stays reachable through `rows[].ids`.
 */
export function groupNotifications(items: readonly NotificationItem[]): NotificationGroup[] {
  type Bucket = { rows: NotificationRow[]; folded: Map<string, NotificationRow>; count: number };
  const groups = new Map<string, Bucket>();
  [...items].sort((a, b) => b.receivedAt - a.receivedAt).forEach(item => {
    const group: Bucket = groups.get(item.appName) ?? { rows: [], folded: new Map<string, NotificationRow>(), count: 0 };
    groups.set(item.appName, group); group.count += 1;
    const signature = foldable(item) ? JSON.stringify([item.title, item.body, item.urgency]) : null;
    const twin = signature === null ? undefined : group.folded.get(signature);
    if (twin) { twin.ids.push(item.id); twin.count += 1; return; }
    const row: NotificationRow = { key: item.id, ids: [item.id], count: 1, item };
    group.rows.push(row);
    if (signature !== null) group.folded.set(signature, row);
  });
  return [...groups].map(([name, group]) => ({ name, count: group.count, rows: group.rows }));
}
