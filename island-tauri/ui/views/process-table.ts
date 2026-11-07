import type { ProcessRow } from '../ipc/bindings';

export type ProcessSort = 'cpu' | 'memory' | 'gpu' | 'name' | 'pid';

/** A process plus its descendants merged under one app row (e.g. all of Firefox). */
export type ProcessGroup = { key: string; lead: ProcessRow; members: ProcessRow[]; cpu: number | null; memory: number; gpu: number | null; vram: number | null };

const sum = (values: (number | null)[]) => values.some(v => v !== null) ? values.reduce<number>((a, v) => a + (v ?? 0), 0) : null;

/** Filter by name, command line or pid (case-insensitive). */
export function matches(row: ProcessRow, query: string): boolean {
  const q = query.trim().toLowerCase();
  return !q || row.name.toLowerCase().includes(q) || row.command.toLowerCase().includes(q) || String(row.pid) === q;
}

/** Group each process with the children that share its name (browser/electron helpers), so
 *  one row is one app. Different-named children (a shell's programs) stay separate. */
export function groupByApp(rows: ProcessRow[]): ProcessGroup[] {
  const byPid = new Map(rows.map(r => [r.pid, r]));
  const root = (row: ProcessRow): ProcessRow => {
    let current = row;
    for (let depth = 0; depth < 64; depth++) {
      const parent = byPid.get(current.ppid);
      if (!parent || parent.name !== row.name) return current;
      current = parent;
    }
    return current;
  };
  const groups = new Map<number, ProcessRow[]>();
  for (const row of rows) { const lead = root(row); groups.set(lead.pid, [...(groups.get(lead.pid) ?? []), row]); }
  return [...groups.entries()].map(([pid, members]) => {
    const lead = byPid.get(pid)!;
    return { key: `${pid}:${lead.startTicks}`, lead, members, cpu: sum(members.map(m => m.cpu)), memory: members.reduce((a, m) => a + m.memoryBytes, 0),
      gpu: sum(members.map(m => m.gpu)), vram: sum(members.map(m => m.vramBytes)) };
  });
}

export function sortGroups(groups: ProcessGroup[], by: ProcessSort): ProcessGroup[] {
  const value = (g: ProcessGroup) => by === 'cpu' ? g.cpu ?? -1 : by === 'memory' ? g.memory : by === 'gpu' ? g.gpu ?? -1 : 0;
  return [...groups].sort((a, b) => by === 'name' ? a.lead.name.localeCompare(b.lead.name) || a.lead.pid - b.lead.pid
    : by === 'pid' ? a.lead.pid - b.lead.pid : value(b) - value(a) || b.memory - a.memory || a.lead.pid - b.lead.pid);
}

export const percent = (value: number | null) => value === null ? '–' : value < 0.05 ? '0%' : value < 10 ? `${value.toFixed(1)}%` : `${Math.round(value)}%`;
