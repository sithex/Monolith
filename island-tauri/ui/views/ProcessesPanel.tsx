import { useEffect, useMemo, useState } from 'react';
import type { KillSignal, ProcessList } from '../ipc/bindings';
import { processKill, processList } from '../ipc/client';
import { formatSize } from '../island/components/AppPrimitives';
import { createVisiblePoller } from './visible-poller';
import { groupByApp, matches, percent, sortGroups, type ProcessGroup, type ProcessSort } from './process-table';

const COLUMNS: { id: ProcessSort; label: string }[] = [
  { id: 'name', label: 'Process' }, { id: 'cpu', label: 'CPU' }, { id: 'memory', label: 'Memory' }, { id: 'gpu', label: 'GPU' },
];

/** Monitor → Processes. `/proc` is read only while the tab is visible (2 s polling); rows are
 *  grouped per app (helpers sharing the parent's name), sortable, filterable, and the user's
 *  own processes can be ended (SIGTERM) or force-killed (SIGKILL) after a confirm step. */
export function ProcessesPanel({ active }: { active: boolean }) {
  const [list, setList] = useState<ProcessList | null>(null), [error, setError] = useState('');
  const [sort, setSort] = useState<ProcessSort>('cpu'), [query, setQuery] = useState('');
  const [grouped, setGrouped] = useState(true), [open, setOpen] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ key: string; signal: KillSignal } | null>(null);
  const [notice, setNotice] = useState('');
  const [poller] = useState(() => createVisiblePoller(processList, value => { setList(value); setError(''); },
    e => setError(String(e)), cb => setTimeout(cb, 2000)));
  useEffect(() => () => poller.dispose(), [poller]);
  useEffect(() => { poller.setVisible(active); }, [active, poller]);
  useEffect(() => { if (!notice) return; const t = setTimeout(() => setNotice(''), 3500); return () => clearTimeout(t); }, [notice]);

  const groups = useMemo(() => {
    if (!list) return [];
    const rows = list.rows.filter(r => matches(r, query));
    const base: ProcessGroup[] = grouped ? groupByApp(rows) : rows.map(r => ({ key: `${r.pid}:${r.startTicks}`, lead: r, members: [r], cpu: r.cpu, memory: r.memoryBytes, gpu: r.gpu, vram: r.vramBytes }));
    return sortGroups(base, sort).slice(0, 150);
  }, [list, query, grouped, sort]);

  async function end(group: ProcessGroup, signal: KillSignal) {
    setConfirm(null);
    // End children first so a parent that respawns helpers does not race the group kill.
    const targets = [...group.members].filter(m => m.own).sort((a, b) => b.pid - a.pid);
    const failures: string[] = [];
    for (const t of targets) { try { await processKill(t.pid, t.startTicks, signal); } catch (e) { failures.push(String(e)); } }
    setNotice(failures[0] ?? `${signal === 'kill' ? 'Force-killed' : 'Asked to quit:'} ${group.lead.name}${targets.length > 1 ? ` (${targets.length} processes)` : ''}`);
  }

  const showGpu = list?.gpuAvailable ?? false;
  return <section className="processes" aria-label="Processes">
    <div className="processes-bar">
      <input type="search" placeholder="Filter by name, command or PID" value={query} onChange={e => setQuery(e.target.value)} aria-label="Filter processes" />
      <div className="segmented" role="group" aria-label="Grouping">
        <button aria-pressed={grouped} onClick={() => setGrouped(true)}>Apps</button>
        <button aria-pressed={!grouped} onClick={() => setGrouped(false)}>All</button>
      </div>
    </div>
    {error && <p role="alert">{error}</p>}
    {notice && <p className="processes-notice" role="status">{notice}</p>}
    <div className="process-table" role="table" data-gpu={showGpu}>
      <div className="process-head" role="row">
        {COLUMNS.filter(c => c.id !== 'gpu' || showGpu).map(c => <button key={c.id} role="columnheader" aria-sort={sort === c.id ? (c.id === 'name' ? 'ascending' : 'descending') : 'none'}
          data-active={sort === c.id} onClick={() => setSort(c.id)}>{c.label}</button>)}
        <span aria-hidden />
      </div>
      {!list && !error && <p className="muted">Reading processes…</p>}
      {groups.map(g => {
        const expanded = open === g.key, own = g.members.some(m => m.own), asking = confirm?.key === g.key;
        return <div key={g.key} className="process-row" role="row" data-open={expanded}>
          <button className="process-name" role="cell" title={g.lead.command} onClick={() => setOpen(expanded ? null : g.key)}>
            <strong>{g.lead.name}</strong>
            <small>{g.members.length > 1 ? `${g.members.length} processes` : `PID ${g.lead.pid}`}{!own && ' · system'}</small>
          </button>
          <span role="cell" className="num" data-hot={(g.cpu ?? 0) >= 25}>{percent(g.cpu)}</span>
          <span role="cell" className="num">{formatSize(g.memory)}</span>
          {showGpu && <span role="cell" className="num" title={g.vram ? `${formatSize(g.vram)} VRAM` : undefined} data-hot={(g.gpu ?? 0) >= 25}>{g.gpu === null && g.vram === null ? '–' : percent(g.gpu ?? 0)}</span>}
          <span role="cell" className="process-actions">
            {asking
              ? <><button onClick={() => setConfirm(null)}>Keep</button>
                  <button className="danger" onClick={() => void end(g, confirm!.signal)}>{confirm!.signal === 'kill' ? 'Force kill' : 'End'}</button></>
              : own && <><button title="Ask it to quit (SIGTERM)" onClick={() => setConfirm({ key: g.key, signal: 'term' })}>End</button>
                  <button className="danger-soft" title="Kill immediately (SIGKILL); unsaved work is lost" onClick={() => setConfirm({ key: g.key, signal: 'kill' })}>Kill</button></>}
          </span>
          {expanded && <div className="process-detail" role="cell">
            <code>{g.lead.command}</code>
            <p className="muted">PID {g.lead.pid} · parent {g.lead.ppid} · {g.members.reduce((a, m) => a + m.threads, 0)} threads{g.vram ? ` · ${formatSize(g.vram)} VRAM` : ''}</p>
            {g.members.length > 1 && <ul>{g.members.slice(0, 40).map(m => <li key={m.pid}><span>{m.pid}</span><span>{m.name}</span><span className="num">{percent(m.cpu)}</span><span className="num">{formatSize(m.memoryBytes)}</span></li>)}</ul>}
          </div>}
        </div>;
      })}
      {list && groups.length === 0 && <p className="muted">No process matches “{query}”.</p>}
    </div>
    <p className="muted">CPU is a share of all {list?.cpuCount ?? ''} cores. Memory is private resident memory. Other users' processes are read-only.</p>
  </section>;
}
