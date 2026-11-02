import type { AgentUpdate, CostLine, Dashboard, IslandIntents, Service } from '../models';
import { duration } from './TimerApp';
import { Icon } from './Icon';
const money = (value: number | null) => value === null ? 'Unavailable' : new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(value);
function CostCard({ title, lines }: { title: string; lines: CostLine[] }) {
  const max = Math.max(1, ...lines.map(line => line.costUsd ?? 0));
  return <div className="card"><h3>{title}</h3>{lines.length ? <ul className="cost-lines">{lines.map(line => <li key={line.key}><span>{line.key}</span><span className="cost-meter" aria-hidden="true"><i style={{ width: `${(line.costUsd ?? 0) / max * 100}%` }}/></span><span className="muted">{money(line.costUsd)}</span></li>)}</ul> : <p className="muted">Unavailable</p>}</div>;
}
export function AiDashboard({ dashboard }: { dashboard: Service<Dashboard> }) {
  if (!dashboard.available) return <div className="card unavailable">Unavailable · {dashboard.reason}</div>;
  const d = dashboard.value;
  const providers = [...new Set(d.limits.map(l => l.provider))];
  const maxTokens = Math.max(1, ...d.hourly.map(h => h.tokens ?? 0));
  return <div className="dashboard">
    <div className="dashboard-grid">{providers.map((provider, index) => <div className={`card limit-card tone-${index % 2 ? 'blue' : 'amber'}`} key={provider}><h3><Icon name={index % 2 ? 'terminal' : 'spark'}/>{provider}</h3>{d.limits.filter(l => l.provider === provider).map(limit => <div className="limit-window" key={limit.label}><div><span>{limit.label} <small className="muted">{limit.resetText ?? (limit.resetsInS === null ? 'Reset unknown' : `Resets in ${duration(limit.resetsInS * 1000)}`)}</small></span><strong>{limit.usedPercent === null ? 'Unavailable' : `${limit.usedPercent}%`}</strong></div>{limit.usedPercent !== null && <progress aria-label={`${provider} ${limit.label} usage`} value={limit.usedPercent} max={100}/>}</div>)}</div>)}</div>
    {!providers.length && <div className="card muted">Provider limits unavailable</div>}
    <div className="dashboard-grid"><div className="card"><h3>Spending <small className="muted">Today · API value</small></h3><div className="spending">{money(d.spendTodayUsd)}</div><p className="muted">{d.spendTodayPartial ? 'Partial · some rows have no known cost' : 'Known usage costs'}</p></div><div className="card now-card"><h3><span className="status-dot online"/>Now</h3>{d.active ? <><strong>{d.active.model}</strong><p className="muted">{d.active.provider}</p><small className="muted">Session {d.active.sessionId}</small></> : <p className="muted">No current session reported</p>}</div></div>
    <div className="card"><h3>Trend <small className="muted">Hourly tokens · supplied cohort</small></h3><div className="trend" role="img" aria-label="Hourly known token trend">{d.hourly.map(hour => <div key={hour.startS} title={`${hour.tokens ?? 'Unknown'} tokens · ${hour.sessions} sessions`}><i data-unknown={hour.tokens === null} style={{ height: `${hour.tokens === null ? 4 : Math.max(2, hour.tokens / maxTokens * 100)}%` }}/></div>)}</div><div className="trend-caption muted"><span>Earlier</span><span>Later</span></div><details><summary>Trend data</summary><ul>{d.hourly.map(h => <li key={h.startS}>Hour {h.startS}: {h.tokens ?? 'Unknown'} tokens, {h.sessions} sessions</li>)}</ul></details></div>
    <div className="dashboard-grid"><CostCard title="Models" lines={d.byModel}/><CostCard title="Projects" lines={d.byProject}/></div>
    <div className="card activity-card"><h3>Activity <small className="muted">Sessions by weekday / hour</small></h3><div className="heatmap" role="img" aria-label="Activity: seven weekdays by twenty-four hours">{d.heatmap.flatMap((row, day) => row.map((count, hour) => <span key={`${day}-${hour}`} title={`Day ${day}, hour ${hour}: ${count} sessions`} data-level={count === 0 ? 0 : count < 2 ? 1 : count < 4 ? 2 : 3}/>))}</div><p className="muted">{d.rowsIgnored} invalid or ignored rows</p></div>
  </div>;
}
export function AgentRunChips({ update, intents }: { update: AgentUpdate; intents: IslandIntents }) {
  return <div className="agent-chips" aria-label="Agent runs">{update.chips.map(chip => <button className="agent-chip" key={chip.id} onClick={() => intents.agent(chip.id)}><span className="status-dot online"/><span><strong>{chip.label}</strong><small>{chip.model ?? 'Unknown model'} · {duration(Math.max(0, chip.elapsedSecs) * 1000)}{chip.isDelegated && ' · delegated'}</small></span><span className="child-count" aria-label={`${chip.children} children`}>{chip.children} ↳</span></button>)}{update.overflow > 0 && <button className="overflow-chip" aria-label={`Show ${update.overflow} more agent runs`} onClick={() => intents.agent(null)}>+{update.overflow}</button>}{!update.chips.length && update.overflow === 0 && <span className="muted">No active runs reported</span>}</div>;
}
