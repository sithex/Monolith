import { useEffect, useState } from 'react';
import { aiUsage, aiConnect, type AiProvider } from '../ipc/client';
import { useAi, aiStore } from '../stores';
import { AiDashboard } from '../island/components/AiDashboard';
import { AiInsights } from '../island/components/AiInsights';
import { dashboardFromUsage, insightsFromUsage, type AiPeriod } from './adapters';
import { useGamePollingActive } from './game-polling';
import { createVisiblePoller } from './visible-poller';
import { AgentLiveView, AgentLiveUsage } from './AgentLiveView';
import '../island/ai-stats.css';
let requestRevision = 0;
/** Last AI Agents page, kept while the island runs (like Monitor's tab). */
let lastPage: 'live' | 'usage' = 'live';
export function AiStatsView({ active }: { active: boolean }) {
  active = useGamePollingActive(active);
  const [page, setPageState] = useState<'live' | 'usage'>(lastPage);
  const setPage = (p: 'live' | 'usage') => { lastPage = p; setPageState(p); };
  const tabs = <div className="segmented ai-stats-tabs" role="tablist" aria-label="AI Agents sections">
    <button role="tab" aria-selected={page === 'live'} aria-pressed={page === 'live'} onClick={() => setPage('live')}>Live</button>
    <button role="tab" aria-selected={page === 'usage'} aria-pressed={page === 'usage'} onClick={() => setPage('usage')}>Usage</button>
  </div>;
  return <div className="ai-agents" aria-label="AI Agents">
    {tabs}
    {page === 'live' ? <AgentLiveView active={active}/> : <>
      <AiUsageView active={active}/>
      <AgentLiveUsage active={active}/>
    </>}
  </div>;
}

/** Nick's existing Overview / Sessions dashboard, unchanged inside Usage. */
export function AiUsageView({ active }: { active: boolean }) {
  active = useGamePollingActive(active);
  const data = useAi(), [busy, setBusy] = useState(false), [notice, setNotice] = useState('');
  const [period, setPeriod] = useState<AiPeriod>('today');
  const [tab, setTab] = useState<'overview' | 'sessions'>('overview');
  const [now, setNow] = useState(Date.now);
  const [poller] = useState(() => createVisiblePoller(async () => {
    const revision = ++requestRevision;
    setBusy(true);
    try { return { value: await aiUsage(), revision }; }
    finally { setBusy(false); }
  }, ({ value, revision }) => { aiStore.apply(value, revision); setNow(Date.now()); },
  error => setNotice(String(error)), callback => window.setTimeout(callback, 60_000), timer => window.clearTimeout(timer as number)));
  useEffect(() => { poller.setVisible(active); }, [active, poller]);
  useEffect(() => () => poller.dispose(), [poller]);
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const clock = window.setInterval(() => setNow(Date.now()), 1000);
    return () => { window.clearInterval(clock); };
  }, [active]);
  const connect = (provider: AiProvider) => {
    void aiConnect(provider).then(result => setNotice(result.launched ? 'Complete sign-in in the secure Hermes terminal. Launch does not prove sign-in.' : result.error ?? 'Sign-in unavailable')).catch(error => setNotice(String(error)));
  };
  return <div className="ai-stats" aria-label="AI usage statistics" aria-busy={busy}>
    <div className="segmented ai-stats-tabs" role="group" aria-label="AI stats section">
      {([['overview', 'Overview'], ['sessions', 'Sessions']] as const).map(([id, name]) => <button key={id} type="button" aria-pressed={tab === id} onClick={() => setTab(id)}>{name}</button>)}
    </div>
    {tab === 'overview'
      ? <AiDashboard dashboard={dashboardFromUsage(data, period, now)} period={period} onPeriodChange={setPeriod} onConnect={connect} now={now}/>
      : <AiInsights insights={insightsFromUsage(data, period)} period={period} onPeriodChange={setPeriod}/>}
    {notice && <p role="status">{notice}</p>}
  </div>;
}
