import { useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { useReducedMotionAll } from './motion-preference';
import type { PackageUpdate, UpdatesChanged } from '../../ipc/bindings';

// Browser-only CSS import also leaves this component usable by the Node SSR checks.
if (typeof document !== 'undefined') void import('../updates.css');

export function versionDifference(oldVersion: string, newVersion: string) {
  let prefix = 0;
  while (prefix < oldVersion.length && prefix < newVersion.length && oldVersion[prefix] === newVersion[prefix]) prefix++;
  return { common: newVersion.slice(0, prefix), changed: newVersion.slice(prefix) };
}

// Deliberately conservative: epochs, pkgrel suffixes, VCS versions and downgrades
// are not semantic-version promises. Only classify pure dotted-numeric increases.
export function versionBump(oldVersion: string, newVersion: string): string | null {
  const dotted = /^\d+\.\d+(?:\.\d+)*$/;
  if (!dotted.test(oldVersion) || !dotted.test(newVersion)) return null;
  const before = oldVersion.split('.').map(BigInt), after = newVersion.split('.').map(BigInt);
  if (before.length !== after.length) return null;
  const index = before.findIndex((value, i) => value !== after[i]);
  if (index < 0 || index > 2 || after[index]! <= before[index]!) return null;
  return ['Major', 'Minor', 'Patch'][index] ?? null;
}

export function checkedLabel(ageSecs: number | null): string {
  if (ageSecs === null || !Number.isFinite(ageSecs) || ageSecs < 0) return 'No successful check yet';
  if (ageSecs < 60) return 'Checked just now';
  if (ageSecs < 3600) return `Checked ${Math.floor(ageSecs / 60)} min ago`;
  if (ageSecs < 86400) return `Checked ${Math.floor(ageSecs / 3600)} hr ago`;
  return `Checked ${Math.floor(ageSecs / 86400)} days ago`;
}

export function groupUpdates(items: PackageUpdate[], query: string) {
  const needle = query.trim().toLocaleLowerCase();
  return ['repo', 'aur', ...new Set(items.map(item => item.source).filter(source => source !== 'repo' && source !== 'aur'))]
    .map(source => ({ source, items: items.filter(item => item.source === source && `${item.name} ${item.arch ?? ''} ${item.oldVersion} ${item.newVersion} ${item.source}`.toLocaleLowerCase().includes(needle)) }))
    .filter(group => group.items.length > 0);
}

type Backend = UpdatesChanged['backend'];

// Group heading per update source: `title` + muted `via`, `aria` for the region label.
export function sourceLabel(source: string): { title: string; via: string; aria: string } {
  if (source === 'repo') return { title: 'Official repositories', via: 'pacman', aria: 'Official repositories' };
  if (source === 'aur') return { title: 'AUR', via: 'via paru', aria: 'AUR via paru' };
  if (source === 'dnf') return { title: 'Fedora repositories', via: 'dnf', aria: 'Fedora repositories · dnf' };
  return { title: source, via: '', aria: source };
}

// Commands offered for copying; the island itself never runs them.
export function updateCommands(backend: Backend): { label: string; command: string }[] {
  if (backend === 'dnf') return [{ label: 'Fedora repositories · dnf', command: 'sudo dnf upgrade --refresh' }];
  if (backend === 'unsupported') return [];
  return [{ label: 'AUR + repositories · paru', command: 'paru -Syu' }, { label: 'Repositories only · pacman', command: 'sudo pacman -Syu' }];
}

// Multilib (dnf) lists one row per name.arch; show the arch only where a name repeats.
function displayName(item: PackageUpdate, repeated: Set<string>) {
  return item.arch && repeated.has(item.name) ? `${item.name}.${item.arch}` : item.name;
}

function RefreshGlyph() {
  return <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5M6.1 7a7 7 0 0 1 11.7-1L20 9M4 15l2.2 3A7 7 0 0 0 18 17" /></svg>;
}

export function UpdatesApp({ snapshot, onRefresh }: { snapshot: UpdatesChanged; onRefresh: () => Promise<unknown> }) {
  const reduced = useReducedMotionAll();
  const [query, setQuery] = useState(''), [checking, setChecking] = useState(false), [actionError, setActionError] = useState('');
  const [commandsOpen, setCommandsOpen] = useState(false), [copied, setCopied] = useState('');
  const busy = useRef(false);
  const backend: Backend = snapshot.backend ?? 'pacman';
  const groups = groupUpdates(snapshot.items, query);
  const healthy = snapshot.available && !snapshot.error && !snapshot.stale && snapshot.count !== null;
  const empty = healthy && snapshot.count === 0;
  const failure = snapshot.error ?? (!snapshot.available ? snapshot.detail : null);
  const age = checkedLabel(snapshot.ageSecs);
  async function refresh() {
    if (busy.current) return;
    busy.current = true; setChecking(true); setActionError('');
    try { await onRefresh(); }
    catch (error) { setActionError(`Couldn't check · ${String(error)}`); }
    finally { busy.current = false; setChecking(false); }
  }
  async function copy(command: string) {
    setCopied(''); setActionError('');
    try {
      if (!navigator.clipboard?.writeText) throw Error('Clipboard unavailable. Select and copy the command below.');
      await navigator.clipboard.writeText(command); setCopied(command);
    } catch (error) { setActionError(String(error)); }
  }
  if (backend === 'unsupported') return <section className="updates-app" aria-label="Package updates">
    <header className="updates-hero"><div className="updates-hero-copy"><span className="updates-eyebrow">Package updates</span>
      <h2><span>Not supported</span></h2>
      <p className="updates-status" role="status">Not supported on this distro</p>
    </div></header>
    <div className="updates-unknown"><span aria-hidden="true">–</span><h3>Update checks aren’t available here</h3><p>The island reads updates with pacman on Arch-based systems and dnf on Fedora. Nothing is checked on this system.</p></div>
  </section>;
  const seen = new Set<string>(), repeated = new Set<string>();
  for (const item of snapshot.items) (seen.has(item.name) ? repeated : seen).add(item.name);
  const commands = updateCommands(backend);
  let rowIndex = 0;
  return <section className="updates-app" aria-label="Package updates">
    <header className="updates-hero">
      <div className="updates-hero-copy"><span className="updates-eyebrow">Package updates</span>
        <h2><span className="updates-total">{snapshot.count ?? '?'}</span> <span>{snapshot.count === 1 ? 'update' : 'updates'}</span></h2>
        <p className={`updates-status${failure ? ' updates-status-error' : ''}`} role="status">
          {checking ? 'Checking…' : failure ? `Couldn't check · ${failure}` : empty ? <><span aria-hidden="true">✓</span> Up to date</> : age}
        </p>
        {(failure || empty) && snapshot.ageSecs !== null && <p className="updates-age">{age}</p>}
        {snapshot.stale && <span className="updates-stale">Stale · showing last known results</span>}
      </div>
      <button type="button" className={`updates-refresh${checking ? ' is-checking' : ''}`} aria-label="Check updates" title="Check updates" aria-busy={checking} disabled={checking} onClick={() => void refresh()}><RefreshGlyph /></button>
    </header>
    {actionError && <p className="updates-action-error" role="alert">{actionError}</p>}
    {snapshot.needsReboot && <p className="updates-reboot"><span aria-hidden="true">↻</span> A restart may be needed after updating</p>}
    {empty && <div className="updates-empty"><span className="updates-check" aria-hidden="true">✓</span><h3>Everything is up to date</h3><p>You’re all set. We’ll keep an eye on the next updates.</p></div>}
    {!empty && snapshot.items.length === 0 && <div className="updates-unknown"><span aria-hidden="true">{failure ? '!' : '…'}</span><h3>{failure ? 'Let’s try that again' : 'Waiting for a package check'}</h3><p>{failure ? 'Your packages are unchanged. Refresh to request the latest status.' : 'The update count will appear after a successful check.'}</p></div>}
    {snapshot.items.length > 8 && <label className="updates-search"><svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></svg><input type="search" aria-label="Filter packages" placeholder="Find a package…" value={query} onChange={event => setQuery(event.target.value)} /></label>}
    {snapshot.items.length > 0 && <div className="updates-packages" tabIndex={0} role="region" aria-label="Available package updates">
      {groups.map(group => {
        const knownCount = group.source === 'repo' || group.source === 'dnf' ? snapshot.repoCount : group.source === 'aur' ? snapshot.aurCount : null;
        const ignoredCount = group.items.filter(item => item.ignored).length;
        const label = sourceLabel(group.source);
        return <section className="updates-group" key={group.source} aria-label={label.aria}>
          <header><h3><span className={`updates-source-icon ${group.source === 'aur' ? 'is-aur' : ''}`} aria-hidden="true">{group.source === 'aur' ? '◇' : '▦'}</span>{label.title}<small>{label.via}</small></h3><span className="updates-group-count">{query.trim() ? `${group.items.length} shown` : knownCount ?? '?'}{ignoredCount > 0 ? ` · ${ignoredCount} ignored` : ''}</span></header>
          <ul>{group.items.map(item => {
            const difference = versionDifference(item.oldVersion, item.newVersion), bump = versionBump(item.oldVersion, item.newVersion);
            const delay = Math.min(rowIndex++, 8) * .01;
            return <motion.li className={`updates-row${item.ignored ? ' is-ignored' : ''}`} key={`${item.source}:${item.name}.${item.arch ?? ''}`} initial={reduced ? false : { opacity: 0, y: 3 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: reduced ? 0 : .1, delay: reduced ? 0 : delay }}>
              <div className="updates-package"><strong>{displayName(item, repeated)}</strong><span className="updates-versions">{item.oldVersion ? <span>{item.oldVersion}</span> : <span title="Installed version unknown">?</span>}<span className="updates-arrow" aria-label="to">→</span><span className="updates-new">{difference.common}<mark>{difference.changed}</mark></span></span></div>
              {item.ignored ? <span className="updates-bump">Ignored</span> : bump && <span className="updates-bump" title="Numeric version change, not a compatibility guarantee">{bump}</span>}
            </motion.li>;
          })}</ul>
        </section>;
      })}
      {groups.length === 0 && <p className="updates-no-results">No packages match “{query}”.</p>}
    </div>}
    <footer className="updates-footer"><span>Review and install in your terminal</span><button type="button" className="updates-action" aria-expanded={commandsOpen} onClick={() => setCommandsOpen(value => !value)}>Update… <span aria-hidden="true">↗</span></button></footer>
    {commandsOpen && <div className="updates-commands"><p>The island never installs packages. Copy the command for your installed package manager, then review it in a terminal.</p>{commands.map(({ label, command }) => <div className="updates-command" key={command}><div><span>{label}</span><code>{command}</code></div><button type="button" aria-label={`Copy ${command}`} onClick={() => void copy(command)}>{copied === command ? 'Copied ✓' : 'Copy'}</button></div>)}<p role="status">{copied ? 'Command copied. Nothing has been run.' : 'No system changes are made here.'}</p></div>}
  </section>;
}
