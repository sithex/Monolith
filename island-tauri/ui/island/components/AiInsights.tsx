import type { Service } from '../models';
import type { AiInsightsData, AiPeriod } from '../../views/adapters';

// "Sessions" tab of AI Stats: per-session cost, cache efficiency, workload and the priciest
// sessions. Reads `local.insights` only; every number is a session-start cohort like the overview.
const money = (v: number | null) => v === null ? '—' : new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: v < 1 ? 3 : 2 }).format(v);
const compact = (v: number | null) => v === null ? '—' : new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 }).format(v);
const pct = (v: number | null) => v === null ? '—' : `${(v * 100).toFixed(v >= 0.995 || v < 0.1 ? 1 : 0)}%`;
const span = (s: number | null) => {
  if (s === null) return '—';
  const m = Math.round(s / 60);
  return m >= 1440 ? `${Math.floor(m / 1440)}d ${Math.floor(m / 60) % 24}h` : m >= 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m}m`;
};
const ratio = (a: number | null, b: number | null) => a !== null && b ? a / b : null;
const sourceName: Record<string, string> = { tui: 'Terminal UI', cli: 'CLI', local: 'Desktop app', gui: 'GUI', desktop: 'Desktop', oneshot: 'One-shot', subagent: 'Subagents', a2a: 'A2A peers', cron: 'Cron' };

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return <div className="ai-stat" title={hint}><dt>{label}</dt><dd>{value}</dd></div>;
}

export function AiInsights({ insights, period, onPeriodChange }: { insights: Service<AiInsightsData>; period: AiPeriod; onPeriodChange?: (p: AiPeriod) => void }) {
  if (!insights.available) return <div className="ai-dashboard"><section className="ai-card ai-empty">{insights.reason}</section></div>;
  const d = insights.value, label = period === 'today' ? 'Today' : period === 'week' ? '7 days' : '30 days';
  const prompt = d.prompt ?? 0;
  const parts = [
    { key: 'read', label: 'Cache read', value: d.cacheRead },
    { key: 'write', label: 'Cache write', value: d.cacheWrite },
    { key: 'fresh', label: 'Uncached input', value: d.input },
  ];
  const topPeak = Math.max(0.001, ...d.top.map(t => t.cost ?? 0));
  const sourcePeak = Math.max(1, ...d.sources.map(s => s.sessions));
  return <div className="ai-dashboard ai-insights">
    <div className="ai-two-columns">
      <section className="ai-card ai-per-session">
        <div className="ai-card-header"><h3>Per session</h3><select aria-label="Insights period" value={period} onChange={e => onPeriodChange?.(e.target.value as AiPeriod)}><option value="today">Today</option><option value="week">7 days</option><option value="month">30 days</option></select></div>
        <div className="ai-spend-value">{money(d.avgCost)}<small>average</small></div>
        <dl className="ai-stats-list">
          <Stat label="Median" value={money(d.medianCost)} hint="Half of priced sessions cost less than this"/>
          <Stat label="Priciest" value={money(d.maxCost)}/>
          <Stat label="Per API call" value={money(d.costPerCall)} hint="Priced-session API value ÷ their model calls"/>
        </dl>
      </section>
      <section className="ai-card ai-cache">
        <div className="ai-card-header"><h3>Prompt cache</h3><small>{compact(d.prompt)} prompt</small></div>
        <div className="ai-spend-value">{pct(d.cacheHit)}<small>served from cache</small></div>
        <div className="ai-cache-bar" role="img" aria-label={`Prompt tokens: ${parts.map(p => `${p.label} ${compact(p.value)}`).join(', ')}`}>
          {prompt > 0 && parts.map(p => (p.value ?? 0) > 0 && <i key={p.key} className={`ai-cache-${p.key}`} style={{ flex: (p.value ?? 0) / prompt }} title={`${p.label}: ${compact(p.value)} (${pct(ratio(p.value, prompt))})`}/>)}
        </div>
        <dl className="ai-stats-list ai-cache-legend">
          {parts.map(p => <div className="ai-stat" key={p.key}><dt><b className={`ai-cache-${p.key}`}/>{p.label}</dt><dd>{compact(p.value)}</dd></div>)}
        </dl>
      </section>
    </div>
    <div className="ai-two-columns">
      <section className="ai-card">
        <div className="ai-card-header"><h3>Sessions</h3><small>{label}</small></div>
        <div className="ai-spend-value">{d.used}<small>with model calls</small></div>
        <dl className="ai-stats-list">
          <Stat label="Priced (API value)" value={String(d.priced)}/>
          <Stat label="Subscription included" value={String(d.included)} hint="Codex sessions on a ChatGPT plan: no API-value price"/>
          {d.unpriced > 0 && <Stat label="Unpriced" value={String(d.unpriced)} hint="Custom endpoints with no known price"/>}
          <Stat label="Subagents" value={String(d.subagents)}/>
          {d.empty > 0 && <Stat label="Opened, no calls" value={String(d.empty)}/>}
        </dl>
      </section>
      <section className="ai-card">
        <div className="ai-card-header"><h3>Workload</h3><small>{label}</small></div>
        <dl className="ai-stats-list">
          <Stat label="API calls" value={compact(d.calls)}/>
          <Stat label="Tool calls" value={compact(d.toolCalls)}/>
          <Stat label="Messages" value={compact(d.messages)}/>
          <Stat label="Calls / session" value={ratio(d.calls, d.used)?.toFixed(0) ?? '—'}/>
          <Stat label="Output written" value={compact(d.output)}/>
          <Stat label="Reasoning" value={compact(d.reasoning)}/>
          <Stat label="Avg length" value={span(d.avgDuration)} hint="First message to last activity"/>
          <Stat label="Total active span" value={span(d.totalDuration)} hint="Sum of session lengths; overlapping sessions count twice"/>
        </dl>
      </section>
    </div>
    <section className="ai-card ai-top">
      <div className="ai-card-header"><h3>Priciest sessions</h3><small>{label}</small></div>
      {d.top.length ? d.top.map((t, i) => <div className={`ai-top-row ai-${t.family}`} key={`${t.startedAt}-${i}`} title={`${t.title}\n${t.project} · ${t.model} · ${sourceName[t.source] ?? t.source}`}>
        <div className="ai-top-line"><strong>{t.title}</strong><span>{money(t.cost)}</span></div>
        <div className="ai-share-track"><i style={{ width: `${Math.max(3, (t.cost ?? 0) / topPeak * 100)}%` }}/></div>
        <p>{t.model}{t.subagent ? ' · subagent' : ''} · {compact(t.prompt)} in · {compact(t.output)} out · {t.calls ?? '—'} calls · {span(t.duration)}</p>
      </div>) : <p className="ai-empty">No priced sessions</p>}
    </section>
    <section className="ai-card ai-sources">
      <div className="ai-card-header"><h3>Where sessions run</h3><small>sessions · API value</small></div>
      {d.sources.length ? d.sources.map(s => <div className="ai-share" key={s.name}>
        <span>{sourceName[s.name] ?? s.name}</span>
        <div className="ai-share-track"><i style={{ width: `${Math.max(3, s.sessions / sourcePeak * 44)}px` }}/></div>
        <small className="ai-source-count">{s.sessions}</small><small>{money(s.cost)}</small>
      </div>) : <p className="ai-empty">No recorded activity</p>}
    </section>
    <p className="ai-footnote">Averages use sessions with an API-value price. Prompt = uncached input + cache reads + cache writes. Session-start cohorts.</p>
  </div>;
}
