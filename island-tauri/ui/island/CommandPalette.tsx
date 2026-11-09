import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { appApi, clipboardCopy, clipboardList, type ClipSummary, type IslandView } from '../ipc/client';
import { dispatch, useIslandState } from '../ipc/island-state';
import { useSettings } from '../stores';
import { appStores, useApp, useLauncher } from '../stores/apps';
import { SETTINGS_CARDS, sectionName } from '../views/settings-model';
import { openSetting } from '../views/settings/SettingsLink';
import { ACTION_OPTIONS, runSideAction } from './side-actions';
import { searchCommands, nextSelection, type SearchItem } from './command-search';
import './command-palette.css';

export const openCommandPalette = () => window.dispatchEvent(new Event('island-command-palette'));
export function CommandPalette({ fail, onOpenChange }: { fail(error: unknown): void; onOpenChange?(open: boolean): void }) {
  const state = useIslandState(), settings = useSettings(), launcher = useLauncher(), calendar = useApp('calendar');
  // Standalone node tests need no new relative imports; match the fast/ease-out tokens.
  const reduced = useReducedMotion() === true || settings.reduceMotion || document.documentElement.dataset.reduceMotion === 'true'
    || (typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  const keyboard = document.documentElement.dataset.navVia === 'key';
  const duration = Math.min(.15, .15 / Math.min(2, Math.max(.5, (settings.animationSpeed ?? 100) / 100)));
  const transition = { duration: keyboard ? 0 : duration, ease: [.23, 1, .32, 1] as const };
  const [open, setOpen] = useState(false), [query, setQuery] = useState(''), [selection, setSelection] = useState(0);
  const [clips, setClips] = useState<ClipSummary[]>([]), [notice, setNotice] = useState('');
  const input = useRef<HTMLInputElement>(null), dialog = useRef<HTMLDivElement>(null), previous = useRef<HTMLElement | null>(null);
  const isOpen = useRef(open), busy = useRef(false), settingJump = useRef(false), currentState = useRef(state); isOpen.current = open; currentState.current = state;
  const navigate = (view: IslandView) => dispatch({ type: 'selectView', view });
  const close = (restore = true) => {
    isOpen.current = false; setOpen(false);
    if (!restore) return;
    requestAnimationFrame(() => {
      if (currentState.current.mode !== 'large') return;
      if (previous.current?.isConnected && !previous.current.closest('[hidden]')) previous.current.focus();
      else window.dispatchEvent(new Event('island-terminal-focus'));
    });
  };
  useEffect(() => {
    const show = () => {
      if (!isOpen.current) previous.current = document.activeElement as HTMLElement | null;
      isOpen.current = true; setOpen(true); setQuery(''); setSelection(0); setNotice('');
      void dispatch({ type: 'pin' }).catch(fail);
    };
    const chord = (event: KeyboardEvent) => {
      if (!event.isComposing && event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey && event.key.toLowerCase() === 'k') {
        // Capture before xterm receives Ctrl+K (otherwise it deletes the TUI's line).
        document.documentElement.dataset.navVia = 'key';
        event.preventDefault(); event.stopImmediatePropagation();
        if (isOpen.current) close(); else show();
      }
    };
    window.addEventListener('island-command-palette', show);
    window.addEventListener('keydown', chord, true);
    let disposed = false, unlisten: (() => void) | undefined;
    void listen('island-command-palette', show).then(off => { if (disposed) off(); else unlisten = off; }).catch(fail);
    return () => { disposed = true; unlisten?.(); window.removeEventListener('island-command-palette', show); window.removeEventListener('keydown', chord, true); };
  }, []);
  useEffect(() => { onOpenChange?.(open && state.mode === 'large'); }, [open, state.mode, onOpenChange]);
  useEffect(() => { if (state.mode !== 'large') { isOpen.current = false; setOpen(false); } }, [state.mode]);
  useEffect(() => {
    if (!open || state.mode !== 'large') return;
    input.current?.focus();
    let live = true;
    setClips([]);
    if (settings.clipboardHistory) void clipboardList(null, 100).then(rows => { if (live) setClips(rows); }, () => { if (live) setNotice('Clipboard history unavailable'); });
    if (calendar?.available) {
      const startS = Math.floor(Date.now() / 1000);
      void appApi.calendar({ startS, endS: startS + 30 * 86400, localOffsetS: -new Date().getTimezoneOffset() * 60 })
        .then(rows => { if (live) appStores.calendar.apply(rows); }, () => {});
    }
    return () => { live = false; };
  }, [open, state.mode]);
  // Focus non-terminal pages on an explicit open/page switch, never on hover previews.
  useEffect(() => {
    if (state.mode !== 'large' || state.view === 'terminal' || isOpen.current || settingJump.current) return;
    const frame = requestAnimationFrame(() => {
      if (isOpen.current || settingJump.current) return;
      const pane = document.querySelector<HTMLElement>(`[data-pane="${state.view}"] .app-pane:not([hidden])`);
      if (pane?.contains(document.activeElement)) return;
      // Keyboard navigation lands on the first control (its ring shows where you are). A click or
      // swipe focuses the page itself, which draws no ring: one Tab still reaches the first control.
      const target = document.documentElement.dataset.navVia === 'key'
        ? pane?.querySelector<HTMLElement>('input:not(:disabled), button:not(:disabled), textarea, [tabindex="0"]') : null;
      if (target) target.focus(); else if (pane) { pane.tabIndex = -1; pane.focus({ preventScroll: true }); }
    });
    return () => cancelAnimationFrame(frame);
  }, [state.mode, state.view]);
  const items = useMemo(() => {
    const allowed = new Set(launcher?.items.filter(i => i.known && i.enabled).map(i => i.view));
    const result: SearchItem[] = ACTION_OPTIONS.filter(option => option.key !== 'commandPalette' && (option.group !== 'Apps' || option.key === 'appGrid' || option.key === 'open:settings' || allowed.has(option.action.type === 'openApp' ? option.action.view : option.key === 'open:music' ? 'music' : option.key === 'open:mixer' ? 'mixer' : option.key === 'open:timer' ? 'timer' : 'terminal')))
      .map(option => ({ id: option.key, label: option.label, group: option.group === 'Apps' ? 'Pages' : 'Quick actions', run: () => runSideAction(option.action, view => { void navigate(view).catch(fail); }) }));
    for (const card of SETTINGS_CARDS) for (const row of card.rows) {
      if (row.when && !row.when(settings)) continue;
      result.push({ id: `setting:${row.id}`, label: row.label, group: 'Settings', detail: card.title, keywords: `${row.hint ?? ''} ${row.synonyms ?? ''} ${sectionName(card.tab)} ${row.keys.join(' ')}`, run: async () => {
        // The row jump owns focus; generic page-open focus must not win its RAF race.
        settingJump.current = true;
        try {
          await openSetting(row.id);
        } finally { requestAnimationFrame(() => { settingJump.current = false; }); }
      } });
    }
    result.push({ id: 'timer:five', label: 'Start a five-minute timer', group: 'Quick actions', keywords: 'countdown 5 minutes', run: async () => { appStores.timer.apply(await appApi.timerCommand({ type: 'setCountdown', durationMs: 300000 })); } });
    for (const clip of clips) result.push({ id: `clip:${clip.id}`, label: clip.preview.slice(0, 160) || `${clip.kind} clipboard item`, group: 'Clipboard', detail: clip.sourceApp ?? 'Copy to clipboard', run: () => clipboardCopy(clip.id) });
    for (const event of calendar?.agenda.slice(0, 100) ?? []) result.push({ id: `event:${event.source}:${event.uid}:${event.startS}`, label: event.summary, group: 'Calendar', detail: `${new Date(event.startS * 1000).toLocaleString()} ${event.location ?? ''}`, run: () => navigate('calendar') });
    return result;
  }, [launcher, settings, clips, calendar]);
  const results = useMemo(() => searchCommands(items, query), [items, query]);
  const selected = Math.min(selection, Math.max(0, results.length - 1));
  const run = async (item: SearchItem | undefined) => {
    if (!item || busy.current) return;
    busy.current = true;
    try {
      await item.run();
      const navigation = item.group === 'Pages' || item.group === 'Settings' || item.group === 'Calendar';
      close(!navigation);
      if (item.id === 'open:terminal') requestAnimationFrame(() => window.dispatchEvent(new Event('island-terminal-focus')));
    } catch (error) { setNotice(String(error)); } finally { busy.current = false; }
  };
  useEffect(() => {
    if (!open) return;
    const key = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      document.documentElement.dataset.navVia = 'key';
      if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); close(); }
      else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) { event.preventDefault(); event.stopImmediatePropagation(); setSelection(nextSelection(selected, event.key, results.length)); }
      else if (event.key === 'Enter') { event.preventDefault(); event.stopImmediatePropagation(); void run(results[selected]); }
      else if (event.key === 'Tab') { event.preventDefault(); event.stopImmediatePropagation(); input.current?.focus(); }
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [open, results, selected]);
  useEffect(() => { dialog.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }); }, [selected, query]);
  // Presence retains the last child on removal; custom carries the closing input modality.
  return <AnimatePresence custom={{ keyboard, reduced }}>{open && state.mode === 'large' && <motion.div key="palette" className="command-palette-shade" data-command-palette=""
    initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit="closed" transition={transition}
    variants={{ closed: ({ keyboard }: { keyboard: boolean }) => ({ opacity: 0, pointerEvents: 'none' as const, transition: { ...transition, duration: keyboard ? 0 : duration } }) }}
    onPointerDown={event => { if (event.target === event.currentTarget) close(); }}>
    <motion.div className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette" ref={dialog}
      initial={{ scale: reduced || keyboard ? 1 : .98 }} animate={{ scale: 1 }} exit="closed" transition={transition}
      variants={{ closed: ({ keyboard, reduced }: { keyboard: boolean; reduced: boolean }) => ({ scale: reduced || keyboard ? 1 : .98, transition: { ...transition, duration: keyboard ? 0 : duration } }) }}>
      <header><span>Command palette</span><button type="button" aria-label="Close command palette" onClick={() => close()}>Esc</button></header>
      <input ref={input} type="search" role="combobox" aria-label="Search commands" aria-autocomplete="list" aria-expanded="true" aria-controls="command-results" aria-activedescendant={results[selected] ? `command-result-${selected}` : undefined}
        placeholder="Search pages, settings, actions…" value={query} autoComplete="off" spellCheck={false} maxLength={120} onChange={event => { setQuery(event.target.value); setSelection(0); }} />
      <div className="command-results" id="command-results" role="listbox" aria-label="Commands">{results.map((item, i) => <button type="button" tabIndex={-1} key={item.id} id={`command-result-${i}`} role="option" aria-selected={i === selected} onPointerMove={() => { document.documentElement.dataset.navVia = 'pointer'; if (selected !== i) setSelection(i); }} onClick={() => { void run(item); }}>
        <span className="command-result-copy"><strong>{item.label}</strong>{item.detail && <small>{item.detail}</small>}</span><span className="command-group">{item.group}</span>
      </button>)}{!results.length && <p className="command-empty">No matching commands</p>}</div>
      <footer><span>{notice || '↑ ↓ Navigate · Enter Run · Esc Close'}</span><span>{results.length} shown</span></footer>
    </motion.div>
  </motion.div>}</AnimatePresence>;
}
