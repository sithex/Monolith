// Formatting for the Monitor view's "Monolith cost" card. Pure, so it is unit-tested; every
// value is null while unavailable and renders as an em dash, never as a fabricated zero.
import type { MonolithCost } from '../ipc/bindings';

const finite = (n: number | null | undefined): n is number => n != null && Number.isFinite(n);

/** CPU as a percentage of one core, one decimal below 10 %, whole numbers above. */
export function formatCpu(n: number | null | undefined): string {
  if (!finite(n)) return '—';
  return n < 10 ? `${n.toFixed(1)}%` : `${Math.round(n)}%`;
}

/** Bytes as MB below 1 GB, else GB with one decimal (PSS of a WebKit tree is hundreds of MB). */
export function formatMemory(bytes: number | null | undefined): string {
  if (!finite(bytes) || bytes < 0) return '—';
  const mb = bytes / (1024 * 1024);
  return mb < 1024 ? `${Math.round(mb)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

export function formatWakeups(n: number | null | undefined): string {
  if (!finite(n)) return '—';
  return `${n < 100 ? n.toFixed(1) : Math.round(n)}/s`;
}

export interface CostRow { label: string; value: string; hint: string }

/** The card's rows, in display order. */
export function costRows(cost: MonolithCost | null): CostRow[] {
  const procs = cost ? `${cost.processes} processes · ${cost.threads} threads` : 'Reading…';
  return [
    { label: 'Monolith CPU', value: formatCpu(cost?.cpu), hint: procs },
    { label: 'Monolith memory (PSS)', value: formatMemory(cost?.pssBytes), hint: 'shared pages counted once' },
    { label: 'Monolith wakeups', value: formatWakeups(cost?.wakeups), hint: 'context switches, all threads' },
    { label: 'plasmashell CPU', value: formatCpu(cost?.plasmashellCpu), hint: 'where the notch is drawn' },
    { label: 'kwin_wayland CPU', value: formatCpu(cost?.kwinCpu), hint: 'compositing and blur' },
  ];
}

/** Footnote: the averaging window actually covered so far. */
export function windowNote(cost: MonolithCost | null): string {
  if (!cost || !(cost.windowSecs > 0)) return 'Averaging the last 10 s; the first reading takes a moment.';
  return `Average of the last ${Math.round(cost.windowSecs)} s, % of one core. Sampled only while this page is open.`;
}
