import { useCallback, useEffect, useRef, useState } from 'react';
import { aiUsage, aiConnect, type AiProvider } from '../ipc/client';
import { useAi, aiStore } from '../stores';
import { AiDashboard } from '../island/components/AiDashboard';
import { dashboardFromUsage, type AiPeriod } from './adapters';
import '../island/ai-stats.css';
let requestRevision = 0;
export function AiStatsView({ active }: { active: boolean }) {
  const data = useAi(), [busy, setBusy] = useState(false), [notice, setNotice] = useState('');
  const [period, setPeriod] = useState<AiPeriod>('today');
  const [now, setNow] = useState(Date.now);
  const pending = useRef(false), visible = useRef(active);
  visible.current = active;
  const refresh = useCallback(async () => {
    if (!visible.current) return;
    if (pending.current) return;
    const revision = ++requestRevision;
    pending.current = true; setBusy(true);
    try { const value = await aiUsage(); aiStore.apply(value, revision); setNow(Date.now()); }
    catch (error) { setNotice(String(error)); }
    finally { pending.current = false; setBusy(false); }
  }, []);
  useEffect(() => {
    if (!active) return;
    visible.current = true;
    void refresh();
    const poll = window.setInterval(() => { void refresh(); }, 60_000);
    const clock = window.setInterval(() => setNow(Date.now()), 1000);
    return () => { visible.current = false; window.clearInterval(poll); window.clearInterval(clock); };
  }, [active, refresh]);
  const connect = (provider: AiProvider) => {
    void aiConnect(provider).then(result => setNotice(result.launched ? 'Complete sign-in in the secure Hermes terminal. Launch does not prove sign-in.' : result.error ?? 'Sign-in unavailable')).catch(error => setNotice(String(error)));
  };
  return <div className="ai-stats" aria-label="AI usage statistics" aria-busy={busy}>
    <AiDashboard dashboard={dashboardFromUsage(data, period, now)} period={period} onPeriodChange={setPeriod} onConnect={connect} now={now}/>
    {notice && <p role="status">{notice}</p>}
  </div>;
}
