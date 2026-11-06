import { useEffect, useState } from 'react';
import { systemStats } from '../ipc/client';
import { monitorStore, runtimeStore, useMonitor } from '../stores';
import { formatSize } from '../island/components/AppPrimitives';
import { createVisiblePoller } from './visible-poller';
import { useGamePollingActive } from './game-polling';
function Gauge({ label, value, hint }: { label: string; value: number | null; hint: string }) {
  const percent = value !== null && Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : null;
  return <section className="card gauge"><h3>{label}<strong>{percent === null ? 'Unavailable' : `${percent.toFixed(1)}%`}</strong></h3>
    {percent !== null && <progress aria-label={`${label} usage`} value={percent} max={100} />}<p className="muted">{hint}</p></section>;
}
const bytes = (n: number | null | undefined) => n == null ? 'Unavailable' : formatSize(n);
export function MonitorView({ active }: { active: boolean }) {
  active = useGamePollingActive(active);
  const stats = useMonitor(), [error, setError] = useState('');
  const [poller] = useState(() => createVisiblePoller(systemStats, (value, rev) => { monitorStore.apply(value, rev); setError(''); }, error => setError(String(error))));
  useEffect(() => {
    if (runtimeStore.getSnapshot().testHooks) Object.defineProperty(window, '__islandMonitorTest', { configurable: true, value: poller.diagnostics });
    return () => poller.dispose();
  }, [poller]);
  useEffect(() => { poller.setVisible(active); }, [active, poller]);
  return <div className="monitor app-stack" aria-label="System monitor">
    {error && <p role="alert">{error}</p>}
    <p className="muted">{stats ? [stats.hostname, stats.platform, stats.cpuCount === null ? 'CPU count unavailable' : `${stats.cpuCount} CPUs`].filter(Boolean).join(' · ') : 'Reading hardware…'}</p>
    <div className="dashboard-grid"><Gauge label="CPU" value={stats?.cpu ?? null} hint={stats?.uptime == null ? 'Uptime unavailable' : `Uptime ${Math.floor(stats.uptime / 3600)}h ${Math.floor(stats.uptime / 60) % 60}m`} />
      <Gauge label="Memory" value={stats?.memPct ?? null} hint={`${bytes(stats?.memUsed)} of ${bytes(stats?.memTotal)}`} /></div>
    <Gauge label="Disk" value={stats?.disk?.percent ?? null} hint={stats?.disk ? `${bytes(stats.disk.used)} used · ${bytes(stats.disk.available)} available · ${stats.disk.path}` : 'Disk unavailable'} />
    <div className="card"><h3>Load · 1 / 5 / 15 min</h3><p>{stats?.loadAverage?.map(n => n.toFixed(2)).join(' / ') ?? 'Unavailable'}</p><h3>Network · receive / send</h3><p>{stats?.network ? `↓ ${bytes(stats.network.rxBytesPerSecond)}/s · ↑ ${bytes(stats.network.txBytesPerSecond)}/s` : 'Unavailable'}</p><p className="muted">All non-loopback interfaces, including virtual adapters.</p></div>
  </div>;
}
