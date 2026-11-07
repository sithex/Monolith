// AI Agents tab, Live page: Habby's Live dock page (app/assets/habby-metrics.js) as React cards in the
// AiDashboard style. Input = buildView() output from ui/views/agent-live-model.ts; no fetching here.
import { useState, type ReactNode } from 'react';
import { DASH, EST_TIP, Dot, Glyph, Header, Kv, Prov, STATUS_TEXT, Spark, Steps, ago, band, dur, isNum, msFine, pct, tok, usd, waitText, type LiveView } from './AgentLiveParts';

const MATCH_TAG: Record<string, string> = {
  'habby-child': '',
  'gateway-local': 'newest local Hermes session (via the gateway)',
  'newest-live': 'newest live session (not this island’s Hermes)',
  unverified: 'newest session (unverified: no process list)',
  'db-newest': 'newest session in state.db',
  'snapshot-newest': 'most recent session (none is live)',
};

function LiveHeader({ v, now, onCopy }: { v: LiveView; now: number; onCopy: (id: string) => void }) {
  const h = v.header ?? {}, who = v.who ?? {}, st: string | null = h.status ?? null;
  const tag = MATCH_TAG[who.matchedBy] ?? '';
  return <section className={`ai-card al-header al-f-${h.family ?? 'other'}`} aria-label="Live session">
    <div className="al-head-row">
      <Glyph kind="claude" family={h.family}/>
      <span className={`al-chip al-f-${h.family ?? 'other'}`}>{h.model ?? 'no session'}</span>
      {h.team && <span className="al-team" title="Fable administrator with subagents">TEAM</span>}
      <span className="al-grow"/>
      <span className="al-status" title={h.statusProv === 'estimate' ? 'Status estimated from agent.log (plugin snapshot missing or its Hermes process is gone)' : 'Status from the habby-metrics plugin'}>
        <Dot status={st}/>{st ? (STATUS_TEXT[st] ?? st) + (h.statusProv === 'estimate' ? ' (log)' : '') : DASH}
      </span>
      {isNum(h.turnStartedAt) && <time className="al-timer" title="This turn">{dur(now - h.turnStartedAt)}</time>}
    </div>
    <div className="al-head-sub">
      <span className="al-ell" title={h.title ?? undefined}>{h.title ?? (who.id ? 'Untitled session' : 'No live Hermes session found')}{tag && ` · ${tag}`}</span>
      {who.id && <button type="button" className="al-copy" title={`Copy session id ${who.id} (for /resume)`} onClick={() => onCopy(who.id)}>{who.short} ⧉</button>}
    </div>
  </section>;
}

function Alerts({ v }: { v: LiveView }) {
  const list: { level: string; text: string }[] = Array.isArray(v.alerts) ? v.alerts.slice(0, 4) : [];
  if (!list.length) return null;
  return <div className="al-alerts">{list.map((a, i) => <div key={i} role="alert" className={`al-banner al-${a.level === 'crit' ? 'crit' : 'warn'}`}>{a.text}</div>)}</div>;
}

function Activity({ v, now }: { v: LiveView; now: number }) {
  const a = v.activity ?? {};
  return <section className="ai-card al-activity">
    <Header title="Activity" kind="now"><Prov kind={a.text ? a.prov : null}/></Header>
    <div className="al-row"><span className={`al-ell al-strong${a.text ? '' : ' al-na'}`} title={a.text ?? undefined}>{a.text ?? 'No activity'}</span>{isNum(a.since) && <time className="al-timer">{dur(now - a.since)}</time>}</div>
    <Steps steps={a.steps}/>
  </section>;
}

function Context({ v }: { v: LiveView }) {
  const x = v.context ?? {}, [open, setOpen] = useState(false);
  const has = isNum(x.used), p = isNum(x.pct) ? x.pct : 0;
  const breakdown: { label: string; tokens: number }[] = Array.isArray(x.breakdown) ? x.breakdown : [];
  const sum = breakdown.reduce((a, b) => a + b.tokens, 0);
  return <section className="ai-card al-context">
    <Header title="Context" kind="context"><Prov kind={has ? x.prov : null}/></Header>
    {!has ? <p className="ai-empty">Unavailable</p> : <>
      <div className="ai-limit-caption"><span>{tok(x.used)} / {tok(x.max)}</span><strong>{pct(x.pct, 1)}</strong></div>
      <div className={`ai-meter al-band-${band(p)}`} role="progressbar" aria-label="Context window used" aria-valuenow={Math.round(p)} aria-valuemin={0} aria-valuemax={100}><i style={{ width: `${Math.min(100, p)}%` }}/></div>
      <div className="al-row al-small"><span>compactions {isNum(x.compressions) ? x.compressions : DASH}</span>{isNum(x.callsLeft) && <span title="Average prompt growth per call over the recent calls">≈ {x.callsLeft} calls left</span>}</div>
      {breakdown.length > 0 && <button type="button" className="al-link" aria-expanded={open} onClick={() => setOpen(o => !o)}>{open ? 'Hide breakdown' : 'Breakdown'}</button>}
      {open && <Kv rows={breakdown.map(b => [b.label, `${tok(b.tokens)} (${sum ? (b.tokens / sum * 100).toFixed(0) : 0}\u00a0%)`])}/>}
    </>}
  </section>;
}

function Cache({ v, now }: { v: LiveView; now: number }) {
  const x = v.cache ?? {}, warm = isNum(x.expiresAt) && x.expiresAt > now;
  return <section className="ai-card al-cache">
    <Header title="Cache" kind="cache"><Prov kind="estimate"/></Header>
    <div className="al-row"><span className="al-strong"><Dot status={isNum(x.expiresAt) ? (warm ? 'warm' : 'cold') : null}/>{isNum(x.expiresAt) ? (warm ? 'warm' : 'cold') : DASH}</span>{warm && <time className="al-timer" title="Until the prompt cache entry expires (TTL guess)">{dur(x.expiresAt - now)}</time>}<span className="al-grow"/><small>TTL {dur((x.ttlSec ?? 0) * 1000)}</small></div>
    <Kv rows={[
      ['hit session', pct(x.hitSession), 'cache read / prompt tokens, whole session (state.db)'],
      ['hit last', pct(x.hitLast), 'last API call'],
      ['cold re-cost', isNum(x.recost) ? '≈ ' + usd(x.recost) : DASH, 'context × (cache write − cache read) list price'],
      ['last call', ago(x.lastCallAt, now)],
    ]}/>
  </section>;
}

function Turn({ v }: { v: LiveView }) {
  const t = v.turn;
  return <section className="ai-card al-turn">
    <Header title="This turn" kind="turn"><Prov kind={t ? t.prov : null}/></Header>
    {!t ? <p className="ai-empty">No turn yet</p> : <>
      <Kv rows={[
        ['API calls', isNum(t.calls) ? String(t.calls) : DASH],
        ['turn $', usd(t.cost), EST_TIP],
        ['input', tok(t.input), 'prompt tokens this turn (uncached + cache read + cache write)'],
        ['output', tok(t.output)],
        [t.tpsGen ? 'gen tok/s' : 'tok/s', isNum(t.tps) ? '≈ ' + t.tps.toFixed(0) : DASH, t.tpsGen ? 'last call: output tokens / (latency − time to first byte)' : 'last call: output tokens / total latency (includes the time to first byte)'],
        ['TTFB', msFine(t.ttfbMs), 'time to first byte, last call (plugin)'],
      ]}/>
      {Array.isArray(t.latencies) && t.latencies.length > 1 && <div className="al-row al-small"><span>latency/call</span><Spark values={t.latencies} width={110} height={14} label={`latency per API call (last 20): ${t.latencies.map((x: number) => x.toFixed(1) + 's').join(' ')}`}/></div>}
    </>}
  </section>;
}

function Session({ v }: { v: LiveView }) {
  const s = v.session, st = v.stats ?? {};
  if (!s) return <section className="ai-card al-session"><Header title="Session" kind="spend"/><p className="ai-empty">{v.sources?.db && v.sources.db !== 'ok' ? `Unavailable (${v.sources.db})` : 'Unavailable'}</p></section>;
  const t = s.tokens ?? {};
  const segs: [string, number, string][] = [['uncached', t.uncached, 's1'], ['write', t.cacheWrite, 's2'], ['read', t.cacheRead, 's3'], ['out', t.output, 's4']];
  const total = segs.reduce((a, g) => a + (isNum(g[1]) ? g[1] : 0), 0);
  const tools: { name: string; n: number }[] = Array.isArray(st.toolsByName) ? st.toolsByName : [];
  return <section className="ai-card al-session">
    <Header title="Session" kind="spend"><Prov kind="estimate" tip={EST_TIP}/></Header>
    <div className="ai-spend-value al-spend" title={EST_TIP}>{usd(s.cost)}<small>API value · + subagents {usd(s.subCost)}</small></div>
    <div className="ai-split al-segs" role="img" aria-label="Session tokens by kind">{total > 0 && segs.map(([label, n, cls]) => isNum(n) && n > 0 && <i key={cls} className={`al-${cls}`} style={{ flex: n / total }} title={`${label}: ${tok(n)} (${(n / total * 100).toFixed(0)} %)`}/>)}</div>
    <div className="al-legend">{segs.map(([label, n, cls]) => <span key={cls}><i className={`al-${cls}`}/>{label} {tok(n)}</span>)}</div>
    <Kv rows={[
      ['cache saved', isNum(s.savings) ? '≈ ' + usd(s.savings) : DASH, 'cache reads priced at input minus cache-read list price (estimate)'],
      ['tokens', tok(s.total)],
      ['API calls', isNum(st.calls) ? String(st.calls) : DASH],
      ['turns', isNum(st.turns) ? String(st.turns) : DASH, 'user messages in this session'],
      ['avg latency', isNum(st.avgLatencyS) ? st.avgLatencyS.toFixed(1) + '\u00a0s' : DASH, 'mean over the recent API calls in agent.log'],
      ['tools', isNum(s.tools) ? String(s.tools) : DASH],
    ]}/>
    {tools.length > 0 && <p className="al-note" title="tool calls by name">{tools.map(x => `${x.name} ×${x.n}`).join(' · ')}</p>}
    {s.continuations > 0 && <p className="al-note" title="Hermes starts a new session id at each compaction; the Session card sums the whole chain">includes {s.continuations} earlier segment{s.continuations > 1 ? 's' : ''} before compaction</p>}
    <p className="al-note" title={`state.db session_model_usage rows with a task, per task · ${EST_TIP}`}>{s.background ? `background/aux: ${usd(s.background.cost)} (${s.background.tasks.map((x: any) => `${x.task} ${usd(x.cost)}`).join(', ')})` : 'background/aux: none'}</p>
  </section>;
}

function AgentRow({ a, main, now, at }: { a: LiveView; main: boolean; now: number; at: number }) {
  const [open, setOpen] = useState(false);
  const tk = a.tokens, total = tk ? (tk.input ?? 0) + (tk.output ?? 0) + (tk.cacheRead ?? 0) + (tk.cacheWrite ?? 0) : null;
  const ms = isNum(a.ms) ? a.ms + (a.active || main ? now - at : 0) : null;
  const procs: { name: string; alive: boolean | null }[] = Array.isArray(a.procs) ? a.procs : [];
  const doing = a.waiting ? waitText(a.waiting) : a.tool ? `Running ${a.tool.name}${a.tool.preview ? ': ' + a.tool.preview : ''}` : null;
  const since = a.waiting?.since ?? a.tool?.since ?? null;
  return <div className={`al-agent${main ? ' al-main' : ''}${a.fading ? ' al-fading' : ''}`}>
    <button type="button" className="al-agent-line" aria-expanded={open} onClick={() => setOpen(o => !o)}>
      <span className="al-caret" aria-hidden="true">{open ? '▾' : '▸'}</span><Dot status={a.status}/>
      <span className="al-role">{a.role ?? (main ? 'main agent' : 'subagent')}</span>
      {a.model && <span className={`al-chip al-f-${a.family ?? 'other'}`}>{a.model}</span>}
      <span className="al-nums" title={`tokens (in + out + cache) · $ = ${EST_TIP} · elapsed`}><span>{tok(total)}</span><span>{usd(a.cost)}</span>{ms !== null && <span>{dur(ms)}</span>}</span>
    </button>
    {a.goal && <div className="al-goal" title={a.goal}>{a.goal}</div>}
    <div className="al-row al-small">
      {doing ? <span className="al-ell al-doing" title={doing}>{doing}</span> : <span>{a.status ? (STATUS_TEXT[a.status] ?? a.status) : DASH}{a.prov === 'estimate' ? ' (estimated)' : ''}</span>}
      {isNum(since) && <time className="al-timer">{dur(now - since)}</time>}
      <span className="al-grow"/><Spark values={a.spark} label="output tokens per API call (last 20)"/>
    </div>
    {procs.length > 0 && <div className="al-note" title="Processes this agent started in the background (Hermes processes.json); ? = could not check that it still runs">background: {procs.map(p => p.name + (p.alive === null ? '?' : '')).join(' · ')}{procs.length > 1 ? ` (${procs.length})` : ''}</div>}
    {open && <div className="al-detail">
      <Steps steps={a.steps}/>
      <Kv rows={[
        ['in', tok(tk?.input), 'uncached input tokens'], ['out', tok(tk?.output)], ['cache rd', tok(tk?.cacheRead)], ['cache wr', tok(tk?.cacheWrite)],
        ['calls', isNum(a.calls) ? String(a.calls) : DASH, 'API calls'], ['tools', isNum(a.tools) ? String(a.tools) : DASH, 'tool calls'],
        ...(main ? [['turn in', tok(a.turn?.input)], ['turn out', tok(a.turn?.output)], ['turn $', usd(a.turn?.cost), EST_TIP]] as [string, string, string?][] : []),
        ['context', pct(a.ctxPct, 1)],
      ]}/>
      {Array.isArray(a.toolsByName) && a.toolsByName.length > 0 && <p className="al-note">{a.toolsByName.map((x: any) => `${x.name} ×${x.n}`).join(' · ')}</p>}
      {a.background && isNum(a.background.calls) && <p className="al-note" title="Background-review / side-question forks of this session (not part of this turn or the totals above)">background review: {a.background.calls} calls · {usd(a.background.cost)}</p>}
    </div>}
  </div>;
}

function Agents({ v, now }: { v: LiveView; now: number }) {
  const ag = v.agents ?? {}, t = v.totals ?? {}, at = isNum(v.at) ? v.at : now;
  return <section className="ai-card al-agents">
    <Header title="Agents" kind="agents"><Prov kind={ag.main ? ag.main.prov : null}/></Header>
    {!ag.main ? <p className="ai-empty">No live session</p> : <>
      <AgentRow a={ag.main} main now={now} at={at}/>
      {(ag.subs ?? []).map((s: LiveView) => <AgentRow key={s.id} a={s} main={false} now={now} at={at}/>)}
      {ag.earlier > 0 && <p className="al-note">+ {ag.earlier} finished subagent{ag.earlier > 1 ? 's' : ''} earlier this session</p>}
      <div className="al-totals" title={`All agents of this session (main + subagents); cost = ${EST_TIP}`}>
        {([['agents', String(t.agents ?? DASH)], ['active', String(t.active ?? DASH)], ['calls', isNum(t.calls) ? String(t.calls) : DASH], ['tokens', tok(t.tokens)], ['cost', usd(t.cost)]] as const).map(([k, val]) => <span key={k}>{k} <b>{val}</b></span>)}
      </div>
    </>}
  </section>;
}

export function sourcesLine(v: LiveView): string {
  const s = v.sources ?? {};
  return `plugin ${s.plugin ?? 'n/a'} · plan ${s.plan ?? 'n/a'} · ${s.db === 'db busy' ? 'db busy (last view)' : 'db ' + (s.db ?? 'n/a')} · log ${s.log ?? 'n/a'}`;
}

export function AgentLivePage({ view, now, onCopy, note, error, extra }: { view: LiveView | null; now: number; onCopy: (id: string) => void; note?: string; error?: string; extra?: ReactNode }) {
  if (!view) return <div className="ai-dashboard al-live"><section className="ai-card"><Header title="Live" kind="now"/><p className="ai-empty">{error ? `Live data unavailable: ${error}` : 'Loading…'}</p></section>{extra}</div>;
  return <div className="ai-dashboard al-live">
    <LiveHeader v={view} now={now} onCopy={onCopy}/>
    {view.sources?.plugin === 'not installed' && <section className="ai-card al-plugin-empty">
      <Header title="Live metrics plugin not installed" kind="now"/>
      <p className="ai-empty">Detailed activity will appear after you install habby-metrics and start a new Hermes session. Available log and usage estimates are shown below; installation is optional.</p>
    </section>}
    <Alerts v={view}/>
    <Activity v={view} now={now}/>
    <div className="ai-two-columns"><Context v={view}/><Cache v={view} now={now}/></div>
    <div className="ai-two-columns"><Turn v={view}/><Session v={view}/></div>
    <Agents v={view} now={now}/>
    {extra}
    <p className="ai-footnote" title={`$ = ${EST_TIP}`}>{sourcesLine(view)}{error ? ` · refresh failed: ${error}` : ''}{note ? ` · ${note}` : ''}</p>
  </div>;
}
