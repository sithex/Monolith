// Presentation only; no fetching, no message text, React text nodes for all metadata.
import { useState } from 'react';
import { DASH, Dot, Header, Kv, Prov, band, dur, isNum, pct, tok, usd, type LiveView } from './AgentLiveParts';
import { hCost } from './AgentLiveUsage';
export const VS_EST_TIP = 'Claude Code list-price estimate from its transcripts; subscription — not money spent';
export const VS_EST_OUT = 'incl. estimated subagent output (Claude Code writes subagent usage before the reply ends)';
export const API_EQUIV_NOTE = '$ = API-equivalent (informational — subscription)';
const VS_STATUS: Record<string, string> = { working: 'working', waiting: 'waiting for you', idle: 'idle' };
const VS_DOT: Record<string, string> = { working: 'tool', waiting: 'waiting', idle: 'idle' };
const STATUS_TIP: Record<string, string> = {
  exact: 'Status from Claude Code’s session file; tool = name only, never its input',
  estimate: 'Status inferred from the transcript (no session file)',
  quiet: 'No transcript write for over 2 min: idle, permission prompt, or long tool run (cannot tell)',
  stale: 'Session file says busy but no activity for 10 min: possibly a crash',
  unknown: 'Unknown session status word',
};
export function vsCost(s: LiveView): string {
  return isNum(s.cost) ? (s.outEstimated ? '~' : '') + usd(s.cost) + (s.costPartial ? '+' : '') : DASH;
}
export function vsStatusText(s: LiveView): string {
  if (s.statusProv === 'quiet' || s.statusProv === 'stale') return 'idle?';
  if (s.statusProv === 'unknown') return s.statusRaw || '?';
  let text = s.statusRaw || VS_STATUS[s.status] || s.status || DASH;
  if (s.status === 'waiting' && s.waitingFor) text += ': ' + s.waitingFor;
  if (s.tool) text += ' · ' + s.tool;
  return text + (s.statusProv === 'estimate' ? ' (estimated)' : '');
}
function ClaudeRow({ s, now }: { s: LiveView; now: number }) {
  const [open, setOpen] = useState(false);
  const tk = s.tokens ?? {}, label = s.project || `session ${s.short}`, ctxPct = isNum(s.ctx?.pct) ? s.ctx.pct : 0;
  return <div className="al-agent cc-row">
    <button type="button" className="al-agent-line" aria-expanded={open} onClick={() => setOpen(o => !o)}>
      <span className="al-caret" aria-hidden="true">{open ? '▾' : '▸'}</span><Dot status={VS_DOT[s.status] ?? 'idle'}/>
      <span className="cc-src">{s.src || 'Claude Code'}</span>
      <span className="al-role al-ell" title={`Claude Code session ${s.short}`}>{label}</span>
      {s.model && <span className={`al-chip al-f-${s.family ?? 'other'}`} title={s.modelId ?? s.model}>{s.model}</span>}
      <span className="al-nums" title={`context % · $ = ${VS_EST_TIP}; ~ estimated output; + some calls unpriced`}><span>{s.ctx ? pct(s.ctx.pct) : DASH}</span><span>{vsCost(s)}</span></span>
    </button>
    <div className="al-row al-small"><span className="al-ell al-doing" title={STATUS_TIP[s.statusProv] ?? STATUS_TIP.exact}>{vsStatusText(s)}</span><span className="al-grow"/>
      {isNum(s.indexing) && <span>reading {s.indexing}{'\u00a0'}%</span>}
      {isNum(s.since) && <time className="al-timer" title="since the last activity">{dur(now - s.since)}</time>}
    </div>
    {open && <div className="al-detail">
      {s.ctx && <><div className="al-row al-small"><span>context {tok(s.ctx.used)} / {tok(s.ctx.max)}</span><span className="al-grow"/><span>{pct(s.ctx.pct, 1)}</span></div>
        <div className={`ai-meter al-band-${band(ctxPct)}`} role="progressbar" aria-label="Context window used" aria-valuenow={Math.round(ctxPct)} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${Math.max(0, Math.min(100, ctxPct))}%` }}/></div></>}
      <Kv rows={[
        ['in', tok(tk.input), 'uncached input, whole session incl. subagents'], ['out', tok(tk.output), s.outEstimated ? VS_EST_OUT : 'output tokens'],
        ['cache rd', tok(tk.cacheRead)], ['cache wr', tok(tk.cacheWrite), '5-minute and 1-hour writes'],
        ['calls', isNum(s.calls) ? String(s.calls) : DASH, 'deduplicated API calls'], ['hit last', pct(s.hitLast)],
        ['subagents', String(s.subagents || 0)], ['sub $', s.subagents ? (isNum(s.subCost) ? usd(s.subCost) : DASH) : DASH, VS_EST_TIP],
        ['compactions', String(s.compactions || 0)], ['cost', vsCost(s), VS_EST_TIP],
      ]}/>
      {s.outEstimated && <p className="al-note">~ output partly estimated (mid-stream transcript lines)</p>}
      {s.error && <p className="al-note">Transcript unavailable: {s.error}</p>}
    </div>}
  </div>;
}
export function ClaudeCodeCard({ live, now, error }: { live: LiveView | null; now: number; error?: string }) {
  const rows: LiveView[] = Array.isArray(live?.sessions) ? live.sessions : [];
  let empty: string | null = null;
  if (!live) empty = error ? `Claude Code data unavailable: ${error}` : 'Loading…';
  else if (live.unavailable) empty = `Claude Code data unavailable: ${live.unavailable}`;
  else if (live.noFolder) empty = 'No Claude Code folder (~/.claude)';
  else if (!rows.length) empty = 'No live Claude Code session' + (isNum(live.idleHideMin) ? ` (idle > ${live.idleHideMin} min hidden)` : '');
  return <section className="ai-card al-agents cc-card" aria-label="Claude Code sessions">
    <Header title="Claude Code" kind="claude"><Prov kind={rows.length ? 'estimate' : null} tip={`Read-only Desktop, VS Code, CLI. $ = ${VS_EST_TIP}`}/></Header>
    {empty ? <p className="ai-empty cc-empty">{empty}</p> : <>{rows.map(s => <ClaudeRow key={s.id} s={s} now={now}/>)}
      {isNum(live?.more) && live.more > 0 && <p className="al-note">+{live.more} more ({rows.length} shown: waiting, working, then most recent)</p>}</>}
    {live && error && !empty && <p className="al-note">refresh failed: {error}</p>}
  </section>;
}
export const SOURCE_TIP: Record<string, string> = { all: 'Hermes + Claude Code sessions' };
export function HistorySourceChips({ sources, value, onChange }: { sources: [string, string][]; value: string; onChange: (k: string) => void }) {
  return <div className="al-toggle cc-chips" role="tablist" aria-label="History source">{sources.map(([k, label]) =>
    <button type="button" role="tab" key={k} aria-selected={k === value} className={k === value ? 'on' : undefined} title={SOURCE_TIP[k] ?? `only ${label} sessions`} onClick={() => onChange(k)}>{label}</button>)}</div>;
}
function delta(a: unknown, b: unknown): string {
  if (!isNum(a) || !isNum(b) || b === 0) return '';
  const d = (a - b) / b * 100;
  return (d >= 0 ? ' ▲' : ' ▼') + Math.abs(d).toFixed(0) + '\u00a0%';
}
function Tile({ label, cur, prev, prevLabel }: { label: string; cur: LiveView; prev: LiveView; prevLabel: string }) {
  return <div className="cc-tile" title={cur.estOut > 0 ? VS_EST_OUT : undefined}><small>{label}</small><strong>{hCost(cur)}</strong>
    <small>{tok(cur.tokens)} tok · {isNum(cur.sessions) ? cur.sessions : 0} sessions</small>
    <small>vs {prevLabel} {hCost(prev)}{delta(cur.cost, prev.cost)}</small></div>;
}
export function HistoryTotalsCard({ history: h, claudeSrc, progress }: { history: LiveView | null; claudeSrc: boolean; progress?: LiveView | null }) {
  return <section className="ai-card cc-totals"><Header title="Today / this week" kind="spend"><Prov kind={h ? 'estimate' : null} tip={claudeSrc ? VS_EST_TIP : undefined}/></Header>
    {h?.today && h.week ? <div className="cc-tiles"><Tile label="Today" cur={h.today} prev={h.yesterday ?? {}} prevLabel="yesterday"/><Tile label="Last 7 days" cur={h.week} prev={h.lastWeek ?? {}} prevLabel="week before"/></div> : <p className="ai-empty">No sessions</p>}
    <p className="al-note">{API_EQUIV_NOTE}, {claudeSrc ? 'Hermes by session start day, Claude Code by call day' : 'by session start day'}</p>
    {((h?.week?.estOut ?? 0) > 0 || (h?.today?.estOut ?? 0) > 0) && <p className="al-note">~ {VS_EST_OUT}</p>}
    {progress && isNum(progress.pct) && progress.pct < 100 && <p className="al-note">indexing Claude Code transcripts {progress.pct}{'\u00a0'}% ({progress.indexed ?? 0}/{progress.files ?? 0} files)</p>}
    {progress && progress.readErrors > 0 && <p className="al-note">{progress.readErrors} transcript(s) could not be read; retried on the next refresh</p>}
  </section>;
}
