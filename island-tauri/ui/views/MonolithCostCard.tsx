import { useEffect, useState } from 'react';
import type { MonolithCost } from '../ipc/bindings';
import { monolithCost } from '../ipc/client';
import { costRows, windowNote } from './monolith-cost';
import { createVisiblePoller } from './visible-poller';

/** Monolith's own running cost. Reads /proc only while `active` (the Monitor page is showing):
 *  the poller is stopped, and no request is pending, the moment the page is hidden. */
export function MonolithCostCard({ active }: { active: boolean }) {
  const [cost, setCost] = useState<MonolithCost | null>(null), [error, setError] = useState('');
  const [poller] = useState(() => createVisiblePoller(monolithCost,
    value => { setCost(value); setError(''); }, e => setError(String(e)), cb => setTimeout(cb, 2000)));
  useEffect(() => () => poller.dispose(), [poller]);
  useEffect(() => { poller.setVisible(active); }, [active, poller]);
  return <section className="card cost-card" aria-label="Monolith cost">
    <h3>Monolith cost</h3>
    {error && <p role="alert">{error}</p>}
    <dl className="cost-rows">{costRows(cost).map(row => <div key={row.label}>
      <dt>{row.label}<small>{row.hint}</small></dt><dd>{row.value}</dd></div>)}</dl>
    <p className="muted">{windowNote(cost)}</p>
  </section>;
}
