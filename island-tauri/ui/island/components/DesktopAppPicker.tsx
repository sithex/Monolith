import { useEffect, useState } from 'react';
import { desktopAppsList, type DesktopApp } from '../../ipc/client';
import { Dropdown } from './AppPrimitives';

/** The installed-app catalogue is read once per explicit open, never on Settings mount. */
export function DesktopAppPicker({ onPick }: { onPick(app: DesktopApp): void }) {
  const [open, setOpen] = useState(false), [apps, setApps] = useState<DesktopApp[]>([]);
  const [loading, setLoading] = useState(false), [error, setError] = useState(''), [query, setQuery] = useState('');
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true); setError(''); setApps([]); setQuery('');
    void desktopAppsList().then(next => { if (!cancelled) setApps(next); })
      .catch(e => { if (!cancelled) setError(String(e)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [open]);
  return <div className="desktop-app-picker">
    <button type="button" className="link" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Cancel app selection' : 'Choose desktop app…'}</button>
    {open && <div className="settings-text-field" role="group" aria-label="Desktop app picker">
      {loading ? <p className="muted" role="status">Reading installed apps…</p> : error ? <p role="alert">{error}</p> : <>
        <input type="search" aria-label="Filter desktop apps" placeholder="Find an installed app…" value={query} onChange={e => setQuery(e.target.value)} />
        <Dropdown label="Installed desktop app" value={null} placeholder="Choose an app…"
          options={apps.filter(app => app.name.toLocaleLowerCase().includes(query.toLocaleLowerCase())).map(app => ({ value: app.id, label: app.name }))}
          onChange={id => { const app = apps.find(app => app.id === id); if (app) { onPick(app); setOpen(false); } }} />
        {!apps.length && <p className="muted">No desktop apps available.</p>}
      </>}
    </div>}
  </div>;
}
