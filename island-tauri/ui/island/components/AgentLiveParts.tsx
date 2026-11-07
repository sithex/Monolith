// Shared bits of the AI Agents Live / Usage cards: Habby's number formatting (habby-metrics.js) and the
// AiDashboard look (ai-card header + glyph). React text only: every string is a text node.
import type { ReactNode } from 'react';

export type LiveView = Record<string, any>;
export const NBSP = '\u00a0';
export const DASH = '—';
// review M6: every API-equivalent $ figure is an estimate, never money spent
export const EST_TIP = 'Hermes list-price estimate; subscription — not money spent';

export const isNum = (x: unknown): x is number => typeof x === 'number' && Number.isFinite(x);
export function tok(n: unknown): string {
  if (!isNum(n)) return DASH;
  if (n >= 1e9) return (n / 1e9).toFixed(2) + 'B';
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e8 ? 0 : 2) + 'M';
  if (n >= 1e4) return (n / 1e3).toFixed(0) + 'k';
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'k';
  return String(Math.round(n));
}
export function usd(n: unknown): string {
  if (!isNum(n)) return DASH;
  if (n >= 100) return '$' + n.toFixed(0);
  if (n >= 10) return '$' + n.toFixed(1);
  return '$' + n.toFixed(2);
}
export const pct = (n: unknown, d = 0) => isNum(n) ? n.toFixed(d) + NBSP + '%' : DASH;
export function dur(ms: unknown): string {
  if (!isNum(ms) || ms < 0) return DASH;
  if (ms < 1000) return Math.round(ms) + NBSP + 'ms';
  const s = Math.round(ms / 1000);
  if (s < 60) return s + NBSP + 's';
  const m = Math.floor(s / 60);
  if (m < 60) return m + 'm ' + String(s % 60).padStart(2, '0') + 's';
  const h = Math.floor(m / 60);
  if (h < 48) return h + 'h ' + String(m % 60).padStart(2, '0') + 'm';
  return Math.floor(h / 24) + 'd ' + (h % 24) + 'h';
}
// latency figures keep their precision (TTFB 1150 ms must not read "1 s", QA nit 2)
export const msFine = (ms: unknown) => !isNum(ms) || ms < 0 ? DASH : ms < 10000 ? Math.round(ms) + NBSP + 'ms' : (ms / 1000).toFixed(1) + NBSP + 's';
export function clockAt(t: unknown, now: number): string {
  if (!isNum(t)) return DASH;
  const d = new Date(t), hm = String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
  if (d.toDateString() === new Date(now).toDateString()) return hm;
  return ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][d.getDay()] + ' ' + hm;
}
export const ago = (t: unknown, now: number) => isNum(t) ? dur(now - t) + ' ago' : DASH;
export const band = (p: number) => p >= 80 ? 'r' : p >= 50 ? 'a' : 'g';
export const STATUS_TEXT: Record<string, string> = { idle: 'idle', thinking: 'thinking', tool: 'running tool', waiting: 'waiting for you', compacting: 'compacting', error: 'error', running: 'running', done: 'done' };
export function waitText(w: { kind?: string } | null | undefined): string | null {
  if (!w) return null;
  return w.kind === 'approval' ? 'Waiting for your approval' : w.kind === 'clarify' ? 'Waiting for your answer'
    : w.kind === 'sudo' ? 'Waiting for a sudo password' : `Waiting for you (${w.kind ?? 'input'})`;
}

const PATHS: Record<string, ReactNode> = {
  claude: <path d="m8 0 1.7 5.9L16 8l-6.3 1.7L8 16l-1.7-6.3L0 8l6.3-2.1Z" fill="currentColor" stroke="none"/>,
  now: <path d="M0 9h4l2-7 3 12 2-7 2 2h3"/>,
  context: <><rect x="1.5" y="3" width="13" height="10" rx="2"/><path d="M4 6h5M4 8h8M4 10h3"/></>,
  cache: <><ellipse cx="8" cy="3.5" rx="6" ry="2"/><path d="M2 3.5v9c0 1.1 2.7 2 6 2s6-.9 6-2v-9M2 8c0 1.1 2.7 2 6 2s6-.9 6-2"/></>,
  turn: <><circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/></>,
  spend: <><circle cx="8" cy="8" r="6"/><path d="M10 5H7a1.5 1.5 0 0 0 0 3h2a1.5 1.5 0 0 1 0 3H6m2-8v10"/></>,
  agents: <><circle cx="5" cy="5" r="2.2"/><circle cx="11.5" cy="6.5" r="1.8"/><path d="M1 13c.5-2.5 2-3.6 4-3.6s3.5 1.1 4 3.6m1-3c2.2-.4 4 .6 4.5 3"/></>,
  blocks: <path d="M2 14V9h2.5v5m2 0V4h2.5v10m2 0V7h2.5v7"/>,
  top: <path d="M2 4h12M2 8h9M2 12h6"/>,
};
export function Glyph({ kind, family }: { kind: string; family?: string }) {
  return <svg className={`ai-glyph${family ? ` al-f-${family}` : ''}`} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" aria-hidden="true">{PATHS[kind] ?? PATHS.now}</svg>;
}
/** Same markup as AiDashboard's Header (ai-card-header). */
export function Header({ title, kind, children }: { title: string; kind: string; children?: ReactNode }) {
  return <div className="ai-card-header"><h3><Glyph kind={kind}/>{title}</h3>{children}</div>;
}
const PROV_LABEL: Record<string, string> = { official: 'official', exact: 'exact', estimate: 'est.', stale: 'stale' };
const PROV_TIP: Record<string, string> = {
  official: 'Official: Anthropic plan usage (via the Hermes plugin)',
  exact: 'Exact: Hermes counters (state.db / plugin)',
  estimate: 'Estimate: derived (API list prices, agent.log, cache TTL guess)',
  stale: 'Official plan data, but older than 15 min: no alerts, pace not shown',
};
export function Prov({ kind, tip }: { kind: string | null | undefined; tip?: string }) {
  if (!kind) return null;
  const k = PROV_LABEL[kind] ? kind : 'estimate';
  return <span className={`al-prov al-prov-${k}`} title={tip ?? PROV_TIP[k]}>{PROV_LABEL[k]}</span>;
}
export const Dot = ({ status, title }: { status: string | null | undefined; title?: string }) => <i className={`al-dot al-st-${status ?? 'none'}`} title={title}/>;
export function Kv({ rows }: { rows: [string, string, string?][] }) {
  return <dl className="al-kv">{rows.map(([k, v, tip]) => <div key={k} title={tip}><dt>{k}</dt><dd className={v === DASH ? 'al-na' : undefined}>{v}</dd></div>)}</dl>;
}
export function Steps({ steps }: { steps: { tool: string; status?: string; ms?: number | null }[] | null | undefined }) {
  if (!Array.isArray(steps) || !steps.length) return null;
  return <div className="al-steps">{steps.slice(-5).map((s, i) => <span key={i} className={`al-step al-step-${s.status ?? 'ok'}`}>{s.tool}{isNum(s.ms) ? ' ' + dur(s.ms) : ''}</span>)}</div>;
}
export function Spark({ values, width = 60, height = 12, label }: { values: unknown; width?: number; height?: number; label: string }) {
  if (!Array.isArray(values) || values.length < 2) return null;
  const vals = values.map(v => isNum(v) ? v : 0), max = Math.max(...vals, 1e-9);
  const pts = vals.map((v, i) => `${(i / (vals.length - 1) * (width - 2) + 1).toFixed(1)},${(height - 1 - v / max * (height - 2)).toFixed(1)}`).join(' ');
  return <svg className="al-spark" width={width} height={height} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={label}><title>{label}</title><polyline points={pts}/></svg>;
}
