import React from 'react';

function bytes(value) {
  if (!Number.isFinite(value)) return 'Unavailable';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let amount = Math.max(0, value), index = 0;
  while (amount >= 1024 && index < units.length - 1) { amount /= 1024; index++; }
  return `${amount.toFixed(index ? 1 : 0)} ${units[index]}`;
}
function Gauge({ label, value, hint }) {
  const percent = Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : null;
  return <div className="gauge">
    <div className="gauge-head"><span className="gauge-label">{label}</span><span className="gauge-value">{percent === null ? 'Unavailable' : `${percent.toFixed(1)}%`}</span></div>
    <div className="gauge-track" role={percent === null ? undefined : 'progressbar'} aria-label={`${label} usage`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}><div className="gauge-fill" style={{ width: `${percent || 0}%` }} /></div>
    <p className="settings-note">{hint}</p>
  </div>;
}
export default function Monitor({ stats }) {
  if (!stats) return <div className="monitor"><p className="settings-note">Reading hardware…</p></div>;
  const disk = stats.disk;
  const network = stats.network;
  const load = stats.loadAverage;
  return <div className="monitor">
    <div className="monitor-host">{[stats.hostname, stats.platform, Number.isFinite(stats.cpuCount) ? `${stats.cpuCount} CPUs` : null].filter(Boolean).join(' · ') || 'Host unavailable'}</div>
    <div className="monitor-grid">
      <Gauge label="CPU" value={stats.cpu} hint={Number.isFinite(stats.uptime) ? `Uptime ${Math.floor(stats.uptime / 3600)}h ${Math.floor(stats.uptime / 60) % 60}m` : 'Uptime unavailable'} />
      <Gauge label="Memory" value={stats.memPct} hint={`${bytes(stats.memUsed)} of ${bytes(stats.memTotal)}`} />
    </div>
    <Gauge label="Disk" value={disk?.percent} hint={disk ? `${bytes(disk.used)} used · ${bytes(disk.available)} available` : 'Disk unavailable'} />
    {disk && <div className="monitor-sub"><span title={disk.path}>{disk.path} · {bytes(disk.total)} total</span></div>}
    <div className="monitor-details">
      <div><span>Load · 1 / 5 / 15 min</span><strong>{Array.isArray(load) && load.length >= 3 && load.slice(0, 3).every(Number.isFinite) ? load.slice(0, 3).map(value => value.toFixed(2)).join(' / ') : 'Load unavailable'}</strong></div>
      <div><span>Network · receive / send</span><strong>{network && Number.isFinite(network.rxBytesPerSecond) && Number.isFinite(network.txBytesPerSecond) ? `↓ ${bytes(network.rxBytesPerSecond)}/s · ↑ ${bytes(network.txBytesPerSecond)}/s` : 'Network unavailable'}</strong></div>
    </div>
  </div>;
}
