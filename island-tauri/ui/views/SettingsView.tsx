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
import { uiSpring } from '../island/motion-profile';
import { SCHEMES } from '../island/tokens/schemes';
import { DEFAULT_APPEARANCE, cardSpec, exportSettings, normalize, parseImport, resetPatch, rowSpec, searchRows, shownRows, type Tab } from './settings-model';
import { commands, type FontFamily } from '../ipc/bindings';
import { useNotifications } from '../ipc/notifications';
import { ShortcutRecorder } from './settings/ShortcutRecorder';
import { FontPicker } from './settings/FontPicker';
import { AppRulesEditor } from './settings/AppRulesEditor';
import { rulesSummary } from './settings/app-rules';

const message = (e: unknown) => e instanceof Error ? e.message : String(e);
function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <div className="settings-row"><span className="label">{label}{hint && <small>{hint}</small>}</span>{children}</div>;
}
export function Card({ title, children }: { title?: string; children: ReactNode }) {
  return <section className="settings-card">{title && <h3>{title}</h3>}{children}</section>;
}
/**
 * Card title with an inline two-tap Reset ("Reset" → "Reset? Confirm"), no modal. The armed
 * state disarms itself after 4 s or when the pointer leaves the card header.
 */
export function ResetHeader({ title, onReset }: { title: string; onReset?(): Promise<void> }) {
  const [armed, setArmed] = useState(false), [busy, setBusy] = useState(false);
  useEffect(() => { if (!armed) return; const t = setTimeout(() => setArmed(false), 4000); return () => clearTimeout(t); }, [armed]);
  return <header><h3>{title}</h3>
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
    <Row label="Blocked apps" hint={count ? `${count} app${count === 1 ? '' : 's'} never notify` : 'Every app can notify'}>
      <button className="link" onClick={() => { setDraft(value); setOpen(!open); }}>{open ? 'Close' : 'Edit'}</button></Row>
    {open && <div className="denied-editor">
      <textarea aria-label="Blocked notification apps" rows={4} spellCheck={false} value={draft} placeholder="One app name per line, e.g. Discord" onChange={e => setDraft(e.target.value)} />
      <div className="app-footer"><span className="muted">Exact names, one per line</span><span className="title-spacer" />
        <button className="pill-button primary" onClick={() => { onSave(draft === '' ? [] : draft.split('\n').filter(l => l !== '')); setOpen(false); }}>Save</button></div>
    </div>}
  </>;
}
/**
 * Export = JSON to the clipboard (Rust GTK clipboard; no dialog plugin is bundled).
 * Import = paste box → one atomic `settings_patch` (Rust validates every key or none).
 */
function Backup({ settings, appearance, show }: { settings: Settings; appearance: Appearance; show(id: string): boolean }) {
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
    {show('exportSettings') && <Row label="Export settings" hint={status || 'Copies your settings as JSON to the clipboard'}>
      <button className="link" type="button" onClick={() => { void copy(); }}>Copy JSON</button></Row>}
    {show('importSettings') && <Row label="Import settings" hint="Paste exported JSON; applied all at once or not at all">
      <button className="link" type="button" onClick={() => { setOpen(!open); setError(''); }}>{open ? 'Close' : 'Paste…'}</button></Row>}
    {open && show('importSettings') && <div className="settings-import">
      <textarea aria-label="Settings JSON to import" rows={6} spellCheck={false} value={draft} placeholder='{"format":"monolith-settings","settings":{…}}'
        onChange={e => { setDraft(e.target.value); setError(''); }} />
      <div className="app-footer"><span className="muted">Replaces the listed settings; apps and their layout are kept</span><span className="title-spacer" />
        <button className="pill-button primary" type="button" disabled={!draft.trim()} onClick={() => { void apply(); }}>Import</button></div>
    </div>}
    {error && <p className="settings-field-error" role="alert">{error}</p>}
  </>;
}
/**
 * Where the island will land while an offset slider is held. The island itself only moves
 * on release (the slider lives inside it: moving it live made the slider run from the
 * pointer), then glides there. Portaled to <body>: the island clips and transforms its children.
 */
function OffsetGhost({ preview, current }: { preview: { x: number; y: number } | null; current: { x: number; y: number } }) {
  const [box, setBox] = useState<{ x: number; y: number; w: number; h: number; r: number } | null>(null);
  useLayoutEffect(() => {
    if (!preview) { return; }
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
export function SettingsView() {
  const settings = useSettings(), appearance = useAppearance(), palette = usePalette();
  const [error, setError] = useState(''), [tab, setTab] = useState<Tab>('general'), [query, setQuery] = useState('');
  const [offsetPreview, setOffsetPreview] = useState<{ x: number; y: number } | null>(null);
  const [outputs, setOutputs] = useState<string[] | null>(null);
  const [fonts, setFonts] = useState<FontFamily[] | null>(null), [fontError, setFontError] = useState<string | null>(null);
  const [rulesOpen, setRulesOpen] = useState(false);
  const notifications = useNotifications();
  // Never disable controls while a save is in flight (that froze sliders mid-drag);
  // saves are serialized in the store and the newest value wins.
  const patch = (p: SettingsPatch) => { setError(''); void saveSettings(p).catch(e => setError(String(e))); };
  const searching = normalize(query) !== '';
  const visible = shownRows(tab, query, settings), show = (id: string) => visible.has(id);
  const shows = (cardId: string) => cardSpec(cardId).rows.some(row => visible.has(row.id));
  // Monitor names only matter for the per-monitor checklist; asked once when it is shown.
  const wantOutputs = settings.showNotchOnEveryMonitor === true || (settings.companionEnabled !== false && shows('notch'));
  useEffect(() => {
    if (!wantOutputs || outputs !== null) return;
    let live = true;
    displayOutputs().then(names => { if (live) setOutputs(names); }, () => { if (live) setOutputs([]); });
    return () => { live = false; };
  }, [wantOutputs]);
  // Installed fonts (fontconfig, cached in Rust) only once the terminal font row is shown.
  const wantFonts = show('terminalFontFamily');
  useEffect(() => {
    if (!wantFonts || fonts !== null) return;
    let live = true;
    commands.fontFamilies().then(r => { if (live) setFonts(r.status === 'ok' ? r.data : []); }, () => { if (live) setFonts([]); });
    return () => { live = false; };
  }, [wantFonts]);
  // App-name suggestions: apps seen in notifications (newest first), then blocked apps.
  const seenApps = [...notifications.map(n => n.appName), ...(settings.notificationDeniedApps ?? [])];
  const appRules = settings.notificationAppRules ?? [];
  const reset = (cardId: string) => async () => {
    setError('');
    try {
      const defaults = await settingsDefaults();
      await saveSettings(resetPatch(cardId, defaults));
      if (cardSpec(cardId).resetsAppearance && appearance !== DEFAULT_APPEARANCE) await saveAppearance(DEFAULT_APPEARANCE);
    } catch (e) { setError(message(e)); }
  };
  const header = (cardId: string) => {
    const card = cardSpec(cardId);
    return <ResetHeader title={card.title} onReset={card.reset === false ? undefined : reset(cardId)} />;
  };
  const section = (cardId: string, children: ReactNode) => shows(cardId) &&
    <section className="settings-card" aria-label={cardSpec(cardId).title}>{header(cardId)}{children}</section>;
  const toggle = (key: BoolKey, id: string, off = false) => {
    const row = rowSpec(id);
    return show(id) && <Row label={row.label} hint={row.hint}><input className="switch" type="checkbox" aria-label={row.label} checked={off ? settings[key] === true : settings[key] !== false} onChange={e => patch({ [key]: e.target.checked })} /></Row>;
  };
  const range = (key: 'terminalFontSize' | 'hoverDelay' | 'offsetX' | 'offsetY', labelText: string, min: number, max: number, unit: string, step = 1, hint?: string) =>
    <Row label={labelText} hint={hint}><DraftRange aria-label={labelText} min={min} max={max} step={step} value={settings[key]} onCommit={(v, final) => { if (final || !isOffset(key)) patch({ [key]: v }); }}
      {...(isOffset(key) ? { live: false, onDraft: (v: number | null) => setOffsetPreview(v === null ? null : { x: key === 'offsetX' ? v : settings.offsetX, y: key === 'offsetY' ? v : settings.offsetY }) } : {})} />
      <output>{isOffset(key) && offsetPreview ? (key === 'offsetX' ? offsetPreview.x : offsetPreview.y) : settings[key]}{unit}</output></Row>;
  const isOffset = (key: string) => key === 'offsetX' || key === 'offsetY';
  const tabs: [Tab, string][] = [['general', 'General'], ['appearance', 'Appearance'], ['layout', 'Layout']];
  const denied = (settings.notificationDeniedApps ?? []).join('\n');
  // The display-server choice only exists on Linux; the running one is stamped by Rust.
  const isLinux = typeof navigator !== 'undefined' && /Linux/i.test(navigator.userAgent) && !/Android/i.test(navigator.userAgent);
  const runningBackend = document.documentElement.dataset.displayBackend === 'wayland' ? 'wayland' : 'x11';
  const hits = searching ? searchRows(query, settings).length : 1;
  return <div className="settings-panel app-stack" aria-label="Island settings" data-own-privacy="">
    <div className="settings-search" role="search">
      <input type="search" aria-label="Search settings" placeholder="Search settings" value={query} spellCheck={false} autoComplete="off"
        onChange={e => setQuery(e.target.value)}
        onKeyDown={e => { if (e.key === 'Escape' && query) { e.preventDefault(); e.stopPropagation(); setQuery(''); } }} />
      {query && <button type="button" aria-label="Clear search" onClick={() => setQuery('')}>✕</button>}
    </div>
    {!searching && <div className="settings-tabs" role="tablist" aria-label="Settings sections">{tabs.map(([id, name]) =>
      <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
        {tab === id && <motion.span layoutId="settings-tab" className="tab-bg" transition={uiSpring(0.3, 0.15)} />}{name}</button>)}</div>}
    {error && <p role="alert">{error}</p>}
    {searching && hits === 0 && <p className="settings-empty" role="status">No settings match “{query.trim()}”</p>}
    {section('behavior', <>
      {toggle('swipeNavigation', 'swipeNavigation')}
      {toggle('stealth', 'stealth')}
      {toggle('clickSounds', 'clickSounds')}
    </>)}
    {section('shortcuts', <>
      {show('openShortcut') && <Row label={rowSpec('openShortcut').label} hint={rowSpec('openShortcut').hint}>
        <ShortcutRecorder label="Open shortcut" value={settings.openShortcut ?? ''} onSave={value => saveSettings({ openShortcut: value })} /></Row>}
      {show('paletteShortcut') && <Row label={rowSpec('paletteShortcut').label} hint={rowSpec('paletteShortcut').hint}>
        <ShortcutRecorder label="Command palette shortcut" value={settings.paletteShortcut ?? ''} onSave={value => saveSettings({ paletteShortcut: value })} /></Row>}
    </>)}
    {section('gameMode', <>
      {toggle('gameModeAuto', 'gameModeAuto')}
      {toggle('gameMode', 'gameMode', true)}
      {toggle('gameModeDnd', 'gameModeDnd')}
      {toggle('gameModeFreezeCompanion', 'gameModeFreezeCompanion')}
      {toggle('gameModePausePollers', 'gameModePausePollers')}
    </>)}
    {show('terminalCommand') && <TerminalCommandSettings value={settings.terminalCommand ?? ''} onSave={command => saveSettings({ terminalCommand: command })} />}
    {section('notifications', <>
      {toggle('notificationsEnabled', 'notificationsEnabled')}
      {toggle('dnd', 'dnd')}
      {toggle('systemNotificationMirror', 'systemNotificationMirror')}
      {toggle('silenceKdePopups', 'silenceKdePopups', true)}
      {toggle('notificationSound', 'notificationSound')}
      {toggle('approvalsInNotch', 'approvalsInNotch')}
      {toggle('redactBodies', 'redactBodies')}
      {show('deniedApps') && <DeniedApps value={denied} onSave={apps => patch({ notificationDeniedApps: apps })} />}
      {show('appRules') && <Row label={rowSpec('appRules').label} hint={rulesSummary(appRules)}>
        <button className="link" type="button" aria-expanded={rulesOpen} onClick={() => setRulesOpen(!rulesOpen)}>{rulesOpen ? 'Close' : 'Edit'}</button></Row>}
      {show('appRules') && rulesOpen && <AppRulesEditor rules={appRules} seenApps={seenApps} onSave={rules => saveSettings({ notificationAppRules: rules })} />}
    </>)}
    {section('clock', toggle('showDate', 'showDate'))}
    {isLinux && section('display', show('displayBackend') && <Row label="Display server" hint={`Running on ${runningBackend === 'wayland' ? 'native Wayland' : 'X11 (XWayland)'}${(settings.displayBackend ?? 'wayland') !== runningBackend ? ' · applies when the island restarts' : ''}`}>
      <div className="segmented" role="group" aria-label="Display server">
        <button aria-pressed={(settings.displayBackend ?? 'wayland') === 'wayland'} onClick={() => patch({ displayBackend: 'wayland' })}>Wayland</button>
        <button aria-pressed={settings.displayBackend === 'x11'} onClick={() => patch({ displayBackend: 'x11' })}>X11</button></div></Row>)}
    {section('apps', <>
      {toggle('hardwareMonitor', 'hardwareMonitor')}
      {show('arrangeApps') && <Row label="Arrange or hide apps" hint="Hold any app in Apps, then drag it"><span className="muted">In Apps</span></Row>}
    </>)}
    {section('backup', <Backup settings={settings} appearance={appearance} show={show} />)}
    {section('scheme', <>
      {show('scheme') && <SchemePicker value={appearance} settings={settings} patch={patch} onPick={id => { void saveAppearance(id).catch(e => setError(String(e))); }} />}
      {toggle('hermesThemeSync', 'hermesThemeSync', true)}
      {show('overrides') && <Row label="Colour overrides" hint="Accent and background come from the scheme unless overridden"><button className="link" onClick={() => patch({ accent: '', background: '', borderColor: '', panelClockColor: '' })}>Clear overrides</button></Row>}
    </>)}
    {section('surface', <>
      {toggle('translucent', 'translucent')}
      {show('glassStrength') && <Row label="Glass strength" hint="How much of the desktop shows through">
        <DraftRange aria-label="Glass strength" min={0} max={100} step={1} value={settings.glassStrength ?? 55} throttleMs={60}
          onDraft={v => { if (v !== null) document.documentElement.style.setProperty('--glass-strength', String(v / 100)); }}
          onCommit={(v, final) => { if (final) patch({ glassStrength: v }); }} />
        <output>{settings.glassStrength ?? 55}%</output></Row>}
      {toggle('borderEnabled', 'borderEnabled')}
    </>)}
    {section('motion', <>
      {toggle('reduceMotion', 'reduceMotion', true)}
      {show('animationSpeed') && <Row label="Animation speed" hint={rowSpec('animationSpeed').hint}>
        <DraftRange aria-label="Animation speed" min={50} max={200} step={5} value={settings.animationSpeed ?? 100}
          onCommit={(v, final) => { if (final) patch({ animationSpeed: v }); }} />
        <output>{settings.animationSpeed ?? 100}%</output></Row>}
    </>)}
    {shows('notch') && <NotchControls settings={settings} background={settings.background || palette.surface} accent={settings.accent || palette.accent} patch={patch}
      outputs={outputs} show={show} header={header('notch')} />}
    {section('text', <>
      {show('uiFont') && <Row label="UI font"><div className="segmented" role="group" aria-label="UI font">
        <button aria-pressed={settings.uiFont === 'system'} onClick={() => patch({ uiFont: 'system' })}>System</button>
        <button aria-pressed={settings.uiFont === 'openrunde'} onClick={() => patch({ uiFont: 'openrunde' })}>OpenRunde</button></div></Row>}
      {show('terminalFontSize') && range('terminalFontSize', 'Terminal font size', 8, 24, 'px')}
      {show('terminalFontFamily') && <Row label="Terminal font" hint={fontError ?? rowSpec('terminalFontFamily').hint}>
        <FontPicker label="Terminal font" value={settings.terminalFontFamily ?? ''} fonts={fonts} error={fontError}
          onChange={value => { setFontError(null); saveSettings({ terminalFontFamily: value }).catch(e => setFontError(message(e))); }} /></Row>}
      {show('terminalScrollback') && <Row label="Terminal scrollback" hint="Lines kept above the screen">
        <DraftRange aria-label="Terminal scrollback" min={500} max={100000} step={500} value={settings.terminalScrollback ?? 5000}
          onCommit={(v, final) => { if (final) patch({ terminalScrollback: v }); }} />
        <output>{Math.round((settings.terminalScrollback ?? 5000) / 100) / 10}k</output></Row>}
      {show('terminalCursorStyle') && <Row label="Terminal cursor"><div className="segmented" role="group" aria-label="Terminal cursor">
        {(['block', 'bar', 'underline'] as const).map(style => <button key={style} aria-pressed={(settings.terminalCursorStyle ?? 'block') === style}
          onClick={() => patch({ terminalCursorStyle: style })}>{style[0]!.toUpperCase() + style.slice(1)}</button>)}</div></Row>}
      {toggle('terminalCursorBlink', 'terminalCursorBlink')}
    </>)}
    {(show('sideButtons') || show('islandSize')) && <LayoutEditor />}
    {section('position', <>
      {show('offsetX') && range('offsetX', 'Horizontal offset', -400, 400, 'px')}
      {show('offsetY') && range('offsetY', 'Vertical offset', -200, 200, 'px')}
    </>)}
    {(tab === 'layout' || searching) && <OffsetGhost preview={offsetPreview} current={{ x: settings.offsetX, y: settings.offsetY }} />}
  </div>;
}
