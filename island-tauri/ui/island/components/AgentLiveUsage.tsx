// AI Agents tab, Usage page additions: Habby's History cards "5-hour windows (peak %)" and
// "Top sessions this week" (click copies the session id for /resume). Input = buildHistory() output.
import { EST_TIP, Header, Prov, clockAt, isNum, pct, usd, type LiveView } from './AgentLiveParts';

// History figures (QA m5): {cost, calls, unpriced, estOut}; "—" = no call has a price
export function hCost(x: LiveView): string {
  if (isNum(x.calls) && x.calls > 0 && (x.unpriced || 0) >= x.calls) return '—';
  return (x.estOut > 0 ? '~' : '') + usd(x.cost) + (x.unpriced > 0 ? '+' : '');
}

export function BlocksCard({ history, now, blocks: saved, pluginMissing }: { history: LiveView | null; now: number; blocks?: { resetAt: number; peak: number }[]; pluginMissing?: boolean }) {
  const blocks: { resetAt: number; peak: number }[] = (saved ?? (Array.isArray(history?.blocks) ? history.blocks : [])).slice(-12);
  const peak = Math.max(0, ...blocks.map(b => b.peak));
  return <section className="ai-card al-blocks">
    <Header title="5-hour windows" kind="blocks">{blocks.length ? <Prov kind="official"/> : <small>peak %</small>}</Header>
    {blocks.length ? <>
      <div className="ai-bars al-block-bars" role="img" aria-label="Peak plan usage per five-hour window, oldest first">{blocks.map(b => <div className="ai-bar-slot" key={b.resetAt} title={`reset ${clockAt(b.resetAt, now)}: peak ${b.peak.toFixed(0)} %`}>
        <div className={`ai-bar al-band-${b.peak === peak ? 'peak' : b.peak >= 80 ? 'r' : b.peak >= 50 ? 'a' : 'g'}`} style={{ height: `${Math.max(2, Math.min(100, b.peak))}%` }}><i style={{ flex: 1 }}/></div>
      </div>)}</div>
      <div className="ai-axis"><span>{clockAt(blocks[0]?.resetAt, now)}</span><span>max {pct(peak)}</span><span>{clockAt(blocks[blocks.length - 1]?.resetAt, now)}</span></div>
    </> : <p className="ai-empty">{pluginMissing ? 'No plan windows: habby-metrics is not installed. Saved history remains available.' : 'Collecting: recorded from the plugin’s plan.json while this tab is open'}</p>}
  </section>;
}

export function TopSessionsCard({ history, ids, onCopy, loaded }: { history: LiveView | null; ids: (string | null)[]; onCopy: (id: string) => void; loaded?: boolean }) {
  const top: LiveView[] = Array.isArray(history?.top) ? history.top : [];
  return <section className="ai-card al-top">
    <Header title="Top sessions this week" kind="top"><Prov kind={top.length ? 'estimate' : null} tip={EST_TIP}/></Header>
    {top.length ? top.map((t, i) => {
      const id = isNum(t.copy) ? ids[t.copy] ?? null : null;
      const name = (t.title || '—') + (t.segments > 1 ? ` (${t.segments} segments)` : '');
      return id
        ? <button type="button" className="al-top-row" key={i} title={`Click: copy the session id ${id} (for /resume). $ includes subagents and post-compaction segments.`} onClick={() => onCopy(id)}><span className="al-ell">{name}</span><b>{hCost(t)}</b></button>
        : <div className="al-top-row" key={i}><span className="al-ell">{name}</span><b>{hCost(t)}</b></div>;
    }) : <p className="ai-empty">{history ? 'No sessions this week' : loaded ? 'Session history unavailable' : 'Loading…'}</p>}
    {history && <p className="al-note" title={EST_TIP}>cache saved {isNum(history.savings) ? '≈ ' + usd(history.savings) : '—'} · subagent share {pct(history.subShare)}</p>}
  </section>;
}

export function AgentLiveUsageCards({ history, ids, now, onCopy, note, error, pluginMissing, blocks, loaded }: { history: LiveView | null; ids: (string | null)[]; now: number; onCopy: (id: string) => void; note?: string; error?: string; pluginMissing?: boolean; blocks?: { resetAt: number; peak: number }[]; loaded?: boolean }) {
  return <div className="ai-dashboard al-usage">
    <div className="ai-two-columns"><BlocksCard history={history} now={now} blocks={blocks} pluginMissing={pluginMissing}/><TopSessionsCard history={history} ids={ids} onCopy={onCopy} loaded={loaded}/></div>
    {(error || note) && <p className="ai-footnote">{error ? `Hermes history unavailable: ${error}` : ''}{error && note ? ' · ' : ''}{note ?? ''}</p>}
  </div>;
}
