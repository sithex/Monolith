import { useEffect, useLayoutEffect, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { AnimatePresence, motion } from 'framer-motion';
import { displayOutputs, settingsDefaults, terminalCopy, type Appearance, type Settings, type SettingsPatch } from '../ipc/client';
import { saveSettings, saveAppearance, useSettings, useAppearance, usePalette } from '../stores';
import { NotchControls } from '../island/NotchControls';
import { LayoutEditor } from '../island/LayoutEditor';
import { SchemePicker } from '../island/SchemePicker';
import { DraftRange } from '../island/components/AppPrimitives';
import { TerminalCommandSettings } from '../island/TerminalCommandSettings';
import { SCHEMES } from '../island/tokens/schemes';
import { DEFAULT_APPEARANCE, SETTINGS_CARDS, SETTINGS_SECTIONS, SECTION_STORAGE_KEY, cardSpec, exportSettings, normalize, parseImport, resetPatch, searchRows, savedSection, settingLocation, type RowSpec, type Tab } from './settings-model';
import { commands, type FontFamily } from '../ipc/bindings';
import { dispatch } from '../ipc/island-state';
import { useNotifications } from '../ipc/notifications';
import { ShortcutRecorder } from './settings/ShortcutRecorder';
import { FontPicker } from './settings/FontPicker';
import { AppRulesEditor } from './settings/AppRulesEditor';
import { SoundPicker } from './settings/SoundPicker';
import { rulesSummary } from './settings/app-rules';
import { ClipboardPatterns, DisplayChoice, WeatherLocation } from './settings/WidgetControls';
import { takePendingSettingJump } from './settings/SettingsLink';
import { SectionIcon } from './settings/SectionIcon';

const message = (e: unknown) => e instanceof Error ? e.message : String(e);
function Row({ id, label, hint, children }: { id: string; label: string; hint?: string; children: ReactNode }) {
  return <div className="settings-row" data-setting-id={id} data-setting-label={label} tabIndex={-1}><span className="label">{label}{hint && <small>{hint}</small>}</span>{children}</div>;
}
export function Card({ title, children }: { title?: string; children: ReactNode }) {
  return <section className="settings-card">{title && <h3>{title}</h3>}{children}</section>;
}
/** Inline two-tap reset, never a modal. */
export function ResetHeader({ title, onReset }: { title: string; onReset?(): Promise<void> }) {
  const [armed, setArmed] = useState(false), [busy, setBusy] = useState(false);
  useEffect(() => { if (!armed) return; const t = setTimeout(() => setArmed(false), 4000); return () => clearTimeout(t); }, [armed]);
  return <header onPointerLeave={() => setArmed(false)}><h3>{title}</h3>
    {onReset && (armed
      ? <><small>Reset?</small><button type="button" className="link" data-armed="true" aria-label={`Confirm reset ${title}`} disabled={busy}
          onClick={() => { setBusy(true); void onReset().finally(() => { setBusy(false); setArmed(false); }); }}>Confirm</button></>
      : <button type="button" className="link" aria-label={`Reset ${title}`} onClick={() => setArmed(true)}>Reset</button>)}
  </header>;
}
function DeniedApps({ value, onSave }: { value: string; onSave(apps: string[]): void }) {
  const [open, setOpen] = useState(false), [draft, setDraft] = useState(value);
  const count = value ? value.split('\n').length : 0;
  return <>
    <Row id="deniedApps" label="Blocked apps" hint={count ? `${count} app${count === 1 ? '' : 's'} never notify` : 'Every app can notify'}>
      <button className="link" onClick={() => { setDraft(value); setOpen(!open); }}>{open ? 'Close' : 'Edit'}</button></Row>
    {open && <div className="denied-editor">
      <textarea aria-label="Blocked notification apps" rows={4} spellCheck={false} value={draft} placeholder="One app name per line, e.g. Discord" onChange={e => setDraft(e.target.value)} />
      <div className="app-footer"><span className="muted">Exact names, one per line</span><span className="title-spacer" />
        <button className="pill-button primary" onClick={() => { onSave(draft === '' ? [] : draft.split('\n').filter(l => l !== '')); setOpen(false); }}>Save</button></div>
    </div>}
  </>;
}
function Backup({ settings, appearance }: { settings: Settings; appearance: Appearance }) {
  const [open, setOpen] = useState(false), [draft, setDraft] = useState(''), [status, setStatus] = useState(''), [error, setError] = useState('');
  const copy = async () => {
    setError(''); setStatus('');
    try { await terminalCopy(exportSettings(settings, appearance)); setStatus('Copied settings JSON to the clipboard'); }
    catch (e) { setError(message(e)); }
  };
  const apply = async () => {
    setError(''); setStatus('');
    try {
      const { patch, appearance: scheme } = parseImport(draft, SCHEMES.map(s => s.id));
      await saveSettings(patch);
      if (scheme && scheme !== appearance) await saveAppearance(scheme);
      setStatus(`Imported ${Object.keys(patch).length} settings`); setOpen(false); setDraft('');
    } catch (e) { setError(message(e)); }
  };
  return <>
    <Row id="exportSettings" label="Export settings" hint={status || 'Copies your settings as JSON to the clipboard'}>
      <button className="link" type="button" onClick={() => { void copy(); }}>Copy JSON</button></Row>
    <Row id="importSettings" label="Import settings" hint="Paste exported JSON; applied all at once or not at all">
      <button className="link" type="button" onClick={() => { setOpen(!open); setError(''); }}>{open ? 'Close' : 'Paste…'}</button></Row>
    {open && <div className="settings-import">
      <textarea aria-label="Settings JSON to import" rows={6} spellCheck={false} value={draft} placeholder='{"format":"monolith-settings","settings":{…}}'
        onChange={e => { setDraft(e.target.value); setError(''); }} />
      <div className="app-footer"><span className="muted">Replaces the listed settings; apps and their layout are kept</span><span className="title-spacer" />
        <button className="pill-button primary" type="button" disabled={!draft.trim()} onClick={() => { void apply(); }}>Import</button></div>
    </div>}
    {error && <p className="settings-field-error" role="alert">{error}</p>}
  </>;
}
/** Offset sliders preview a ghost and move the real island only on release. */
function OffsetGhost({ preview, current }: { preview: { x: number; y: number } | null; current: { x: number; y: number } }) {
  const [box, setBox] = useState<{ x: number; y: number; w: number; h: number; r: number } | null>(null);
  useLayoutEffect(() => {
    if (!preview) return;
    const island = document.querySelector<HTMLElement>('.island');
    if (!island) return;
    const rect = island.getBoundingClientRect(), r = parseFloat(getComputedStyle(island).borderRadius) || 0;
    const x = Math.max(0, Math.min(innerWidth - rect.width, rect.x + preview.x - current.x));
    const y = Math.max(0, Math.min(innerHeight - rect.height, rect.y + preview.y - current.y));
    setBox({ x, y, w: rect.width, h: rect.height, r });
  }, [preview?.x, preview?.y, current.x, current.y]);
  return createPortal(<AnimatePresence>{preview && box && <motion.div key="ghost" className="offset-ghost" aria-hidden="true"
    style={{ left: box.x, top: box.y, width: box.w, height: box.h, borderRadius: box.r }}
    initial={{ opacity: 0, scale: 0.98 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, transition: { duration: 0.25, delay: 0.12 } }} transition={{ duration: 0.14 }}>
    <span>Release to move · {preview.x > 0 ? '+' : ''}{preview.x}, {preview.y > 0 ? '+' : ''}{preview.y}</span></motion.div>}</AnimatePresence>, document.body);
}

type BoolKey = { [K in keyof Settings]: Settings[K] extends boolean ? K : never }[keyof Settings];
const FLAGS: Partial<Record<string, BoolKey>> = Object.fromEntries(([
  'swipeNavigation', 'stealth', 'clickSounds', 'autostart', 'showInTaskbar', 'showDate', 'notificationsEnabled', 'dnd', 'systemNotificationMirror', 'silenceKdePopups', 'notificationSound', 'approvalsInNotch', 'redactBodies', 'gameModeAuto', 'gameMode', 'gameModeDnd', 'gameModeFreezeCompanion', 'gameModePausePollers', 'hermesThemeSync', 'translucent', 'borderEnabled', 'reduceMotion', 'hardwareMonitor', 'clipboardHistory', 'clipboardPersist', 'weatherEnabled', 'batteryAlerts', 'lyricsEnabled', 'equalizerEnabled', 'terminalCursorBlink',
] as const).map(key => [key, key]));
// Numeric bounds mirror the Rust schema. DraftRange retains local values during saves.
const RANGES: Partial<Record<keyof Settings, [number, number, number, string]>> = {
  hoverDelay: [0, 1200, 10, 'ms'], toastTtlMs: [1500, 20000, 500, 'ms'],
  glassStrength: [0, 100, 1, '%'], animationSpeed: [50, 200, 5, '%'],
  terminalFontSize: [8, 24, 1, 'px'], terminalScrollback: [500, 100000, 500, ' lines'],
  clipboardHistorySize: [10, 500, 1, ' entries'], batteryLowPercent: [5, 50, 1, '%'], batteryCriticalPercent: [1, 30, 1, '%'],
  equalizerBands: [8, 32, 1, ' bands'], offsetX: [-400, 400, 1, 'px'], offsetY: [-200, 200, 1, 'px'],
};
export function SettingsView() {
  const settings = useSettings(), appearance = useAppearance(), palette = usePalette();
  const [error, setError] = useState(''), [tab, setTab] = useState<Tab>(() => { try { return savedSection(localStorage.getItem(SECTION_STORAGE_KEY)); } catch { return 'general'; } }), [query, setQuery] = useState('');
  const [offsetPreview, setOffsetPreview] = useState<{ x: number; y: number } | null>(null);
  const [outputs, setOutputs] = useState<string[] | null>(null), [jumpTarget, setJumpTarget] = useState<string | null>(null), [highlight, setHighlight] = useState<string | null>(null);
  const [fonts, setFonts] = useState<FontFamily[] | null>(null), [fontError, setFontError] = useState<string | null>(null);
  const [rulesOpen, setRulesOpen] = useState(false);
  const notifications = useNotifications();
  const jumpTo = (id: string) => { const target = settingLocation(id); if (target) { setTab(target.card.tab); setQuery(''); setJumpTarget(id); } };
  useEffect(() => { try { localStorage.setItem(SECTION_STORAGE_KEY, tab); } catch { /* Storage may be unavailable. */ } }, [tab]);
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const jump = (event: Event) => { const id = (event as CustomEvent<unknown>).detail; takePendingSettingJump(); if (typeof id === 'string') jumpTo(id); };
    window.addEventListener('island-setting-jump', jump);
    const pending = takePendingSettingJump(); if (pending) jumpTo(pending);
    return () => window.removeEventListener('island-setting-jump', jump);
  }, []);
  useLayoutEffect(() => {
    if (!jumpTarget) return;
    // Stable IDs, not labels: repeated words and custom notch rows are unambiguous.
    const container = document.querySelector<HTMLElement>(`.settings-panel [data-setting-id="${jumpTarget}"]`);
    container?.scrollIntoView({ block: 'center', behavior: settings.reduceMotion || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' });
    const control = container?.querySelector<HTMLElement>('input:not(:disabled), button:not(:disabled), textarea, select') ?? container;
    control?.focus({ preventScroll: true }); setHighlight(jumpTarget); setJumpTarget(null);
  }, [jumpTarget, tab, settings.reduceMotion]);
  useEffect(() => {
    if (!highlight) return;
    const row = document.querySelector<HTMLElement>(`.settings-panel [data-setting-id="${highlight}"]`);
    row?.setAttribute('data-setting-highlight', 'true');
    const timer = setTimeout(() => setHighlight(null), 1200);
    return () => { clearTimeout(timer); row?.removeAttribute('data-setting-highlight'); };
  }, [highlight]);
  const patch = (p: SettingsPatch) => { setError(''); void saveSettings(p).catch(e => setError(message(e))); };
  const searching = normalize(query) !== '';
  const wantOutputs = tab === 'notch';
  useEffect(() => {
    if (!wantOutputs || outputs !== null) return;
    let live = true; displayOutputs().then(names => { if (live) setOutputs(names); }, () => { if (live) setOutputs([]); });
    return () => { live = false; };
  }, [wantOutputs, outputs]);
  const wantFonts = tab === 'terminal';
  useEffect(() => {
    if (!wantFonts || fonts !== null) return;
    let live = true; commands.fontFamilies().then(r => { if (live) setFonts(r.status === 'ok' ? r.data : []); }, () => { if (live) setFonts([]); });
    return () => { live = false; };
  }, [wantFonts, fonts]);
  const seenApps = [...notifications.map(n => n.appName), ...(settings.notificationDeniedApps ?? [])];
  const appRules = settings.notificationAppRules ?? [];
  const reset = (cardId: string) => async () => {
    setError('');
    try {
      const defaults = await settingsDefaults(); await saveSettings(resetPatch(cardId, defaults));
      if (cardSpec(cardId).resetsAppearance && appearance !== DEFAULT_APPEARANCE) await saveAppearance(DEFAULT_APPEARANCE);
    } catch (e) { setError(message(e)); }
  };
  const header = (cardId: string) => { const card = cardSpec(cardId); return <ResetHeader title={card.title} onReset={card.reset === false ? undefined : reset(cardId)} />; };
  const isLinux = typeof navigator !== 'undefined' && /Linux/i.test(navigator.userAgent) && !/Android/i.test(navigator.userAgent);
  const runningBackend = document.documentElement.dataset.displayBackend === 'wayland' ? 'wayland' : 'x11';
  const segmented = (key: 'hourFormat' | 'weatherUnit' | 'terminalCursorStyle' | 'uiFont' | 'displayBackend', label: string, options: [string, string][]) =>
    <div className="segmented" role="group" aria-label={label}>{options.map(([value, name]) => <button key={value} type="button" aria-pressed={settings[key] === value} onClick={() => patch({ [key]: value })}>{name}</button>)}</div>;
  const renderRow = (row: RowSpec) => {
    const { id, label } = row; let control: ReactNode; let hint = row.hint;
    const flag = FLAGS[id], key = row.keys[0], bounds = key && RANGES[key];
    if (flag) control = <input className="switch" type="checkbox" aria-label={label} checked={settings[flag] === true} onChange={e => patch({ [flag]: e.target.checked })} />;
    else if (bounds && key) {
      const [min, max, step, unit] = bounds, offset = key === 'offsetX' || key === 'offsetY';
      const number = Number(settings[key] ?? min);
      control = <><DraftRange aria-label={label} min={min} max={max} step={step} value={number}
        {...(offset ? { live: false, onDraft: (v: number | null) => setOffsetPreview(v === null ? null : { x: key === 'offsetX' ? v : settings.offsetX, y: key === 'offsetY' ? v : settings.offsetY }) } : {})}
        {...(id === 'glassStrength' ? { throttleMs: 60, onDraft: (v: number | null) => { if (v !== null) document.documentElement.style.setProperty('--glass-strength', String(v / 100)); } } : {})}
        onCommit={(v, final) => { if (final) patch({ [key]: v }); }} /><output>{offset && offsetPreview ? (key === 'offsetX' ? offsetPreview.x : offsetPreview.y) : number}{unit}</output></>;
    } else switch (id) {
      case 'hourFormat': control = segmented('hourFormat', label, [['h12', '12 hour'], ['h24', '24 hour']]); break;
      case 'weatherUnit': control = segmented('weatherUnit', label, [['fahrenheit', 'Fahrenheit'], ['celsius', 'Celsius']]); break;
      case 'uiFont': control = segmented('uiFont', label, [['system', 'System'], ['openrunde', 'OpenRunde']]); break;
      case 'terminalCursorStyle': control = segmented('terminalCursorStyle', label, [['block', 'Block'], ['bar', 'Bar'], ['underline', 'Underline']]); break;
      case 'displayBackend':
        if (!isLinux) return <Row key={id} id={id} label={label} hint="Display server selection is available on Linux"><span className="muted">Unavailable on this platform</span></Row>;
        hint = `Running on ${runningBackend === 'wayland' ? 'native Wayland' : 'X11 (XWayland)'}${(settings.displayBackend ?? 'wayland') !== runningBackend ? ' · applies when the island restarts' : ''}`;
        control = segmented('displayBackend', label, [['wayland', 'Wayland'], ['x11', 'X11']]); break;
      case 'displayId': control = <DisplayChoice value={settings.displayId} outputs={outputs} onChange={value => patch({ displayId: value })} />; break;
      case 'openShortcut': case 'paletteShortcut': control = <ShortcutRecorder label={label} value={settings[id] ?? ''} onSave={value => saveSettings({ [id]: value })} />; break;
      case 'terminalCommand': return <div key={id} data-setting-id={id} tabIndex={-1}><TerminalCommandSettings embedded value={settings.terminalCommand ?? ''} onSave={command => saveSettings({ terminalCommand: command })} /></div>;
      case 'terminalFontFamily': hint = fontError ?? hint; control = <FontPicker label={label} value={settings.terminalFontFamily ?? ''} fonts={fonts} error={fontError}
        onChange={value => { setFontError(null); saveSettings({ terminalFontFamily: value }).catch(e => setFontError(message(e))); }} />; break;
      case 'notificationSoundId': control = <SoundPicker label={label} value={settings.notificationSoundId ?? ''} emptyLabel="Each app's own (default)" onChange={id => patch({ notificationSoundId: id })} />; break;
      case 'deniedApps': return <DeniedApps key={id} value={(settings.notificationDeniedApps ?? []).join('\n')} onSave={apps => patch({ notificationDeniedApps: apps })} />;
      case 'appRules': return <div key={id}><Row id={id} label={label} hint={rulesSummary(appRules)}><button className="link" type="button" aria-expanded={rulesOpen} onClick={() => setRulesOpen(!rulesOpen)}>{rulesOpen ? 'Close' : 'Edit'}</button></Row>
        {rulesOpen && <AppRulesEditor rules={appRules} seenApps={seenApps} globalSound={settings.notificationSoundId ?? ''} onSave={rules => saveSettings({ notificationAppRules: rules })} />}</div>;
      case 'scheme': return <div key={id} data-setting-id={id} tabIndex={-1}><SchemePicker value={appearance} settings={settings} patch={patch} onPick={id => { void saveAppearance(id).catch(e => setError(message(e))); }} /></div>;
      case 'overrides': control = <button className="link" onClick={() => patch({ accent: '', background: '', borderColor: '', panelClockColor: '' })}>Clear overrides</button>; break;
      case 'arrangeApps': control = <button className="link" onClick={() => { void dispatch({ type: 'selectView', view: 'launcher' }).catch(e => setError(message(e))); }}>Open Apps</button>; break;
      case 'clipboardDenyPatterns': control = <ClipboardPatterns value={settings.clipboardDenyPatterns ?? []} onSave={saveSettings} />; break;
      case 'weatherLocation': control = <WeatherLocation value={settings.weatherLocation ?? null} onSave={saveSettings} />; break;
      default: throw new Error(`Settings row has no control: ${id}`);
    }
    return <Row key={id} id={id} label={label} hint={hint}>{control}</Row>;
  };
  const results = searching ? searchRows(query, settings) : [];
  return <div className="settings-panel app-stack" aria-label="Island settings" data-own-privacy="" data-reduce-motion={settings.reduceMotion ? 'true' : undefined} data-highlight={highlight ?? ''}>
    <div className="settings-search" role="search"><input type="search" aria-label="Search settings" placeholder="Search all settings" value={query} spellCheck={false} autoComplete="off"
      onChange={e => setQuery(e.target.value)} onKeyDown={e => { if (e.key === 'Escape' && query) { e.preventDefault(); e.stopPropagation(); setQuery(''); } }} />
      {query && <button type="button" aria-label="Clear search" onClick={() => setQuery('')}><svg width="12" height="12" viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" fill="none" stroke="currentColor" strokeWidth="2" /></svg></button>}
    </div>
    {error && <p role="alert">{error}</p>}
    <div className="settings-layout">
      <nav className="settings-rail" aria-label="Settings sections">{SETTINGS_SECTIONS.map(section => <button key={section.id} type="button" data-section={section.id}
        aria-label={section.name} aria-current={tab === section.id ? 'page' : undefined} onClick={() => { setTab(section.id); setQuery(''); }}>
        <SectionIcon section={section.id} /><span className="settings-rail-name" role="tooltip">{section.name}</span>
      </button>)}</nav>
      <div className="settings-content">
        {searching ? <>
          {results.length === 0 && <p className="settings-empty" role="status">No settings match “{query.trim()}”</p>}
          {SETTINGS_SECTIONS.map(section => { const groups = results.filter(group => group.card.tab === section.id); return groups.length > 0 && <section className="settings-results" key={section.id} aria-label={section.name}>
            <h3>{section.name}</h3>{groups.map(group => <div key={group.card.id}><h4>{group.card.title}</h4>{group.rows.map(row => <button type="button" key={row.id} className="settings-result" onClick={() => jumpTo(row.id)}>
              <span>{row.label}</span>{row.hint && <small>{row.hint}</small>}</button>)}</div>)}
          </section>; })}
        </> : <>
          <h2 className="settings-section-title">{SETTINGS_SECTIONS.find(section => section.id === tab)!.name}</h2>
          {SETTINGS_CARDS.filter(card => card.tab === tab).map(card => {
            if (card.id === 'notch' || card.id === 'companion') return <NotchControls key={card.id} settings={settings} background={settings.background || palette.surface} accent={settings.accent || palette.accent} patch={patch}
              outputs={outputs} show={id => card.rows.some(row => row.id === id)} header={header(card.id)} />;
            if (card.id === 'layoutEditor') return <LayoutEditor key={card.id} />;
            return <section key={card.id} className="settings-card" aria-label={card.title}>{header(card.id)}{card.id === 'backup' ? <Backup settings={settings} appearance={appearance} /> : card.rows.map(renderRow)}</section>;
          })}
        </>}
      </div>
    </div>
    {tab === 'island' && <OffsetGhost preview={offsetPreview} current={{ x: settings.offsetX, y: settings.offsetY }} />}
  </div>;
}
